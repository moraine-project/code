use std::sync::Arc;

use crate::blob::BlobStore;
use crate::capability::Capability;
use crate::db::MetadataStore;

#[derive(Clone)]
pub struct AppState {
	pub store: Arc<BlobStore>,
	pub metadata: Arc<MetadataStore>,
	pub capability: Arc<Capability>,
	pub login_limiter: Arc<crate::auth::LoginLimiter>,
	pub metrics: Arc<crate::ops::metrics::Metrics>,
	pub rate_limiter: Arc<crate::auth::ratelimit::RateLimiter>,
	pub web_dir: Option<Arc<std::path::PathBuf>>,
}
