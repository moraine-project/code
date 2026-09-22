use std::sync::Arc;

use crate::auth::ratelimit;
use crate::capability::Capability;
use crate::config::Config;
use crate::db::MetadataStore;
use crate::ops::metrics::Metrics;
use crate::registry::definitions;
use crate::routes::{self, AppState};

pub(super) async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
	config.check_binding()?;
	let store = Arc::new(crate::blob::open_store(&config).await?);
	let metadata = Arc::new(MetadataStore::open_url(&config.database_url()).await?);
	let capability = Arc::new(Capability::discover(&config));
	let state = AppState {
		store,
		metadata,
		capability,
		login_limiter: Arc::new(crate::auth::LoginLimiter::new()),
		metrics: Arc::new(Metrics::new()),
		rate_limiter: Arc::new(ratelimit::RateLimiter::new()),
		web_dir: config.web_dir.clone().map(Arc::new),
	};
	match definitions::load_directory(&state, &config.data_dir.join("definitions")).await {
		Ok(loaded) if loaded > 0 => tracing::info!(loaded, "loaded definition files"),
		Ok(_) => {}
		Err(error) => tracing::warn!(%error, "the definition directory was not loaded"),
	}
	crate::runtime::workers::spawn(state.clone(), config.clone());
	let app = routes::router(state);
	let listener = tokio::net::TcpListener::bind(config.bind).await?;
	tracing::info!(address = %config.bind, "moraine-server listening");
	axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await?;
	Ok(())
}
