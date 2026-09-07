mod auth;
mod blob;
mod capability;
mod config;
mod db;
mod federation;
mod ops;
mod registry;
mod routes;
#[cfg(test)]
mod test_support;
mod verify;

use std::sync::Arc;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::auth::ratelimit;
use crate::config::{Cli, Command};
use crate::db as store;
use crate::federation::webhooks;
use crate::ops::{backup, bootstrap, gc, metrics};
use crate::registry::definitions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	tracing_subscriber::fmt()
		.with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
		.init();

	let cli = Cli::parse();
	let config = cli.config;
	if config.skip_migrate_on_start {
		match store::migrations::pending_url(&config.database_url()).await {
			Ok(0) => {}
			Ok(pending) => {
				return Err(format!("{pending} migration(s) pending; run `moraine-server migrate`").into());
			}
			Err(error) => return Err(error.into()),
		}
	}
	if let Some(Command::Backup { out }) = cli.command {
		return match backup::run(&config, &out).await {
			Ok(summary) => {
				println!("database snapshot: {}", out.join("metadata.sqlite").display());
				println!("blob inventory:    {}", out.join("blobs.txt").display());
				println!(
					"projects: {}  blobs: {}  bytes: {}",
					summary.projects, summary.blobs, summary.bytes
				);
				println!("back up publisher root keys separately; they are not in this backup");
				Ok(())
			}
			Err(error) => Err(error.into()),
		};
	}
	if let Some(Command::Migrate) = cli.command {
		let url = config.database_url();
		return match store::migrations::migrate_url(&url).await {
			Ok(applied) => {
				println!("{}: {} migration(s) applied", config.database_label(), applied);
				Ok(())
			}
			Err(error) => Err(error.into()),
		};
	}
	if let Some(Command::VerifyBackup { dir }) = cli.command {
		return match backup::verify(&dir).await {
			Ok(verified) => {
				println!(
					"backup ok: projects: {}  blobs: {}  bytes: {}",
					verified.projects, verified.blobs, verified.bytes
				);
				Ok(())
			}
			Err(error) => Err(error.into()),
		};
	}
	if let Some(Command::Restore { dir, force }) = cli.command {
		return match backup::restore(&config, &dir, force).await {
			Ok(restored) => {
				println!(
					"restored into {}: projects: {}  blobs: {}  bytes: {}",
					config.data_dir.display(),
					restored.projects,
					restored.blobs,
					restored.bytes
				);
				println!("the webhook signing key is not in a backup; copy it separately or a new one is generated");
				Ok(())
			}
			Err(error) => Err(error.into()),
		};
	}
	if let Some(Command::Bootstrap { email }) = cli.command {
		match bootstrap::run(&config, &email).await {
			Ok(created) => {
				println!("operator account: {}", email.trim().to_lowercase());
				println!("operator id:      {}", created.user_id);
				println!("operator password: {}", created.password);
				if let Some(key) = created.webhook_public_key {
					println!("webhook public key: {key}");
				}
				println!("store the password now; it is not shown again");
				return Ok(());
			}
			Err(error) => return Err(error.into()),
		}
	}
	if let Err(message) = config.check_binding() {
		return Err(message.into());
	}
	let store = Arc::new(blob::open_store(&config).await?);
	let metadata = Arc::new(store::MetadataStore::open_url(&config.database_url()).await?);
	let capability = Arc::new(capability::Capability::discover(&config));
	let state = routes::AppState {
		store,
		metadata,
		capability,
		login_limiter: Arc::new(auth::LoginLimiter::new()),
		metrics: Arc::new(metrics::Metrics::new()),
		rate_limiter: Arc::new(ratelimit::RateLimiter::new()),
		web_dir: config.web_dir.clone().map(Arc::new),
	};
	match definitions::load_directory(&state, &config.data_dir.join("definitions")).await {
		Ok(loaded) if loaded > 0 => tracing::info!(loaded, "loaded definition files"),
		Ok(_) => {}
		Err(error) => tracing::warn!(%error, "the definition directory was not loaded"),
	}
	let worker_state = state.clone();
	let prune_state = state.clone();
	let staging_retention = config.staging_retention_seconds;
	let blob_retention = config.blob_retention_seconds;
	let app = routes::router(state);

	tokio::spawn(async move {
		let mut ticker = tokio::time::interval(std::time::Duration::from_secs(10));
		loop {
			ticker.tick().await;
			if let Err(error) = webhooks::deliver_pending(&worker_state, 20).await {
				tracing::warn!(%error, "webhook dispatch failed");
			}
		}
	});

	let maintenance_interval = config.maintenance_interval_seconds;
	tokio::spawn(async move {
		if maintenance_interval == 0 {
			return;
		}
		let mut ticker = tokio::time::interval(std::time::Duration::from_secs(maintenance_interval));
		loop {
			ticker.tick().await;
			let now = unix_now();
			let notifications_before = now - (moraine_model::event::NOTIFICATION_RETENTION_DAYS * 86_400) as i64;
			let deliveries_before = now - (moraine_model::event::WEBHOOK_RETENTION_DAYS * 86_400) as i64;
			if let Ok(pruned) = prune_state.metadata.prune_notifications(notifications_before).await
				&& pruned > 0
			{
				tracing::info!(pruned, "pruned notifications");
			}
			if let Ok(pruned) = prune_state.metadata.prune_deliveries(deliveries_before).await
				&& pruned > 0
			{
				tracing::info!(pruned, "pruned webhook deliveries");
			}
			if let Ok(synced) = federation::resync_definitions(&prune_state).await
				&& synced > 0
			{
				tracing::info!(synced, "resynced definitions");
			}
			match federation::resync_subscriptions(&prune_state).await {
				Ok(report) if report.synced > 0 || report.failed > 0 => {
					tracing::info!(synced = report.synced, failed = report.failed, "resynced subscriptions");
				}
				Ok(_) => {}
				Err(error) => tracing::warn!(%error, "subscription resync failed"),
			}
			match federation::probe_mirrors(&prune_state, prune_state.capability.max_mirror_probes_per_cycle as i64).await {
				Ok(confirmed) if confirmed > 0 => {
					tracing::info!(confirmed, "confirmed mirror holdings");
				}
				Ok(_) => {}
				Err(error) => tracing::warn!(%error, "mirror probe failed"),
			}
			match gc::collect(&prune_state, staging_retention, blob_retention).await {
				Ok(collected) if collected.staging > 0 || collected.blobs > 0 => {
					tracing::info!(staging = collected.staging, blobs = collected.blobs, "collected storage");
				}
				Ok(_) => {}
				Err(error) => tracing::warn!(%error, "storage collection failed"),
			}
		}
	});

	let listener = tokio::net::TcpListener::bind(config.bind).await?;
	tracing::info!(address = %config.bind, "moraine-server listening");
	axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await?;
	Ok(())
}

fn unix_now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}
