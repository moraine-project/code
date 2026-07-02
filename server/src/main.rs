mod auth;
mod blob;
mod capability;
mod config;
mod federation;
mod password;
mod registry;
mod review;
mod routes;
mod store;
mod verify;

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
	};
	let app = routes::router(state);

	let listener = tokio::net::TcpListener::bind(config.bind).await?;
	tracing::info!(address = %config.bind, "moraine-server listening");
	axum::serve(listener, app).await?;
	Ok(())
}
