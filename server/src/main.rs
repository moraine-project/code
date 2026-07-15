mod accounts;
mod advisories;
mod auth;
mod blob;
mod capability;
mod config;
mod definitions;
mod federation;
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

use crate::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	tracing_subscriber::fmt()
		.with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
		.init();

	let config = Config::parse();
	let store = Arc::new(blob::BlobStore::new(&config.data_dir).await?);
	let metadata = Arc::new(store::MetadataStore::open(config.data_dir.join("metadata.sqlite")).await?);
	let capability = Arc::new(capability::Capability::discover(&config));
	let state = routes::AppState {
		store,
		metadata,
		capability,
		web_dir: config.web_dir.clone().map(Arc::new),
	};
	let worker_state = state.clone();
	let prune_state = state.clone();
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
