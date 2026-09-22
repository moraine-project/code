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
		max_upload_bytes_per_account: 5_368_709_120,
		max_projects: 10_000,
		tls_terminated: false,
		allow_insecure_http: false,
		web_origins: Vec::new(),
		registration: crate::config::Registration::Open,
		max_definitions: 1_000,
		max_sync_entries: 10_000,
		metrics_token: None,
		smtp_url: None,
		mail_from: None,
		public_url: None,
		require_verified_email: false,
		max_mirror_probes_per_cycle: 20,
		max_mirror_probe_bytes: 268_435_456,
		max_feed_page_entries,
		max_feed_scan_pages,
		scanner_enabled: false,
		scanner_provider_id: "local-clamav".to_string(),
		scanner_kind: "clamav".to_string(),
		scanner_command: "clamscan".to_string(),
		scanner_args: Vec::new(),
		scanner_timeout_seconds: 300,
		skip_migrate_on_start: false,
		database_url: Some(database.to_string()),
		max_response_bytes: 16_777_216,
		staging_retention_seconds: 3_600,
		blob_retention_seconds: 604_800,
		max_sync_pages: 200,
		requests_per_minute,
		max_concurrent_syncs: 4,
		maintenance_interval_seconds: 3_600,
		tls_extra_roots: None,
		allow_insecure_federation_local,
		publishing,
		web_dir: None,
		s3: Default::default(),
	}
}
