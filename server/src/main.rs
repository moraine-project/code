mod accounts;
mod advisories;
mod auth;
mod blob;
mod bootstrap;
mod capability;
mod config;
mod definitions;
mod egress;
mod federation;
mod gc;
mod metrics;
mod mirrors;
mod notifications;
mod orgs;
mod password;
mod registry;
mod review;
mod routes;
mod search;
mod store;
#[cfg(test)]
mod test_support;
mod verify;
mod views;
mod webhooks;

use std::sync::Arc;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::config::{Cli, Command};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	tracing_subscriber::fmt()
		.with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
		.init();

	let cli = Cli::parse();
	let config = cli.config;
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
	let store = Arc::new(blob::BlobStore::new(&config.data_dir).await?);
	let metadata = Arc::new(store::MetadataStore::open(config.data_dir.join("metadata.sqlite")).await?);
	let capability = Arc::new(capability::Capability::discover(&config));
	let state = routes::AppState {
		store,
		metadata,
		capability,
		login_limiter: Arc::new(auth::LoginLimiter::new()),
		metrics: Arc::new(metrics::Metrics::new()),
		web_dir: config.web_dir.clone().map(Arc::new),
	};
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

	tokio::spawn(async move {
		let mut ticker = tokio::time::interval(std::time::Duration::from_secs(3600));
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
	axum::serve(listener, app).await?;
	Ok(())
}

fn unix_now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}
