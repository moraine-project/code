use std::sync::Arc;

use axum::Router;

use super::database::{seed_operators, test_database};
use crate::blob::BlobStore;
use crate::capability::Capability;
use crate::db::MetadataStore;
use crate::routes::AppState;

pub(crate) async fn app() -> (Router, tempfile::TempDir) {
	app_mode(crate::config::Publishing::Open, false).await
}

pub(crate) async fn app_review() -> (Router, tempfile::TempDir) {
	app_mode(crate::config::Publishing::Review, false).await
}

pub(crate) async fn app_mode(
	publishing: crate::config::Publishing,
	allow_insecure_federation_local: bool,
) -> (Router, tempfile::TempDir) {
	app_with_limit(publishing, allow_insecure_federation_local, 100).await
}

pub(crate) async fn app_with_limit(
	publishing: crate::config::Publishing,
	allow_insecure_federation_local: bool,
	max_feed_page_entries: u32,
) -> (Router, tempfile::TempDir) {
	app_with_limits(publishing, allow_insecure_federation_local, max_feed_page_entries, 600).await
}

pub(crate) async fn app_with_rate_limit(requests_per_minute: u32) -> (Router, tempfile::TempDir) {
	app_with_limits(crate::config::Publishing::Open, false, 100, requests_per_minute).await
}

pub(crate) async fn app_with_scan_pages(max_feed_scan_pages: u32) -> (Router, tempfile::TempDir) {
	let directory = tempfile::tempdir().expect("tempdir");
	let database = test_database(directory.path()).await;
	let application = app_in_with_scan(
		directory.path(),
		&database,
		crate::config::Publishing::Open,
		false,
		100,
		600,
		max_feed_scan_pages,
	)
	.await;
	(application, directory)
}

pub(crate) async fn app_with_limits(
	publishing: crate::config::Publishing,
	allow_insecure_federation_local: bool,
	max_feed_page_entries: u32,
	requests_per_minute: u32,
) -> (Router, tempfile::TempDir) {
	let directory = tempfile::tempdir().expect("tempdir");
	let database = test_database(directory.path()).await;
	let application = app_in_with_scan(
		directory.path(),
		&database,
		publishing,
		allow_insecure_federation_local,
		max_feed_page_entries,
		requests_per_minute,
		50,
	)
	.await;
	(application, directory)
}

pub(crate) async fn app_in(
	directory: &std::path::Path,
	publishing: crate::config::Publishing,
	allow_insecure_federation_local: bool,
	max_feed_page_entries: u32,
	requests_per_minute: u32,
) -> Router {
	app_in_with_scan(
		directory,
		&crate::db::sqlite_url(&directory.join("metadata.sqlite")),
		publishing,
		allow_insecure_federation_local,
		max_feed_page_entries,
		requests_per_minute,
		50,
	)
	.await
}

pub(crate) async fn app_in_with_scan(
	directory: &std::path::Path,
	database: &str,
	publishing: crate::config::Publishing,
	allow_insecure_federation_local: bool,
	max_feed_page_entries: u32,
	requests_per_minute: u32,
	max_feed_scan_pages: u32,
) -> Router {
	let store = Arc::new(BlobStore::new(directory).await.expect("blob store"));
	let metadata = Arc::new(MetadataStore::open_url(database).await.expect("metadata"));
	seed_operators(&metadata).await;
	let config = test_config(
		directory,
		database,
		publishing,
		allow_insecure_federation_local,
		max_feed_page_entries,
		requests_per_minute,
		max_feed_scan_pages,
	);
	crate::routes::router(test_state(store, metadata, &config))
}

pub(crate) async fn app_with_max_projects(max_projects: u64) -> (Router, tempfile::TempDir) {
	let directory = tempfile::tempdir().expect("tempdir");
	let database = test_database(directory.path()).await;
	let store = Arc::new(BlobStore::new(directory.path()).await.expect("blob store"));
	let metadata = Arc::new(MetadataStore::open_url(&database).await.expect("metadata"));
	seed_operators(&metadata).await;
	let mut config = test_config(
		directory.path(),
		&database,
		crate::config::Publishing::Open,
		false,
		100,
		600,
		50,
	);
	config.max_projects = max_projects;
	(crate::routes::router(test_state(store, metadata, &config)), directory)
}

fn test_state(store: Arc<BlobStore>, metadata: Arc<MetadataStore>, config: &crate::config::Config) -> AppState {
	AppState {
		store,
		metadata,
		capability: Arc::new(Capability::discover(config)),
		branding: std::sync::Arc::new(crate::instance::BrandingSource::default()),
		login_limiter: std::sync::Arc::new(crate::auth::LoginLimiter::new()),
		metrics: std::sync::Arc::new(crate::ops::metrics::Metrics::new()),
		rate_limiter: std::sync::Arc::new(crate::auth::ratelimit::RateLimiter::new()),
		web_dir: None,
	}
}

#[allow(clippy::too_many_arguments)]
fn test_config(
	directory: &std::path::Path,
	database: &str,
	publishing: crate::config::Publishing,
	allow_insecure_federation_local: bool,
	max_feed_page_entries: u32,
	requests_per_minute: u32,
	max_feed_scan_pages: u32,
) -> crate::config::Config {
	crate::config::Config {
		bind: "127.0.0.1:0".parse().expect("addr"),
		data_dir: directory.to_path_buf(),
		max_artifact_bytes: 1024,
		max_feed_page_entries,
		max_feed_scan_pages,
		requests_per_minute,
		database_url: Some(database.to_string()),
		registration: crate::config::Registration::Open,
		allow_insecure_federation_local,
		publishing,
		..crate::config::Config::default()
	}
}
