use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::header;
use axum::response::Response;
use moraine_crypto::{ObjectKind as Kind, SigningKey, object_id};
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use crate::blob::BlobStore;
use crate::capability::Capability;
use crate::db::MetadataStore;
use crate::routes::AppState;

mod wire;
pub(crate) use wire::*;

static DATABASES: std::sync::Mutex<Vec<(std::path::PathBuf, String)>> = std::sync::Mutex::new(Vec::new());

pub(crate) async fn isolated_store() -> Option<MetadataStore> {
	let base = std::env::var("MORAINE_TEST_POSTGRES").ok()?;
	let schema = unique_schema();
	create_schema(&base, &schema).await;
	let separator = if base.contains('?') { '&' } else { '?' };
	let url = format!("{base}{separator}options=-csearch_path%3D{schema}");
	Some(MetadataStore::open_url(&url).await.expect("store"))
}

pub(crate) async fn store_for(directory: &std::path::Path) -> MetadataStore {
	let url = DATABASES
		.lock()
		.expect("databases")
		.iter()
		.find(|(path, _)| path == directory)
		.map(|(_, url)| url.clone())
		.unwrap_or_else(|| crate::db::sqlite_url(&directory.join("metadata.sqlite")));
	MetadataStore::open_url(&url).await.expect("store")
}

async fn test_database(directory: &std::path::Path) -> String {
	let Ok(base) = std::env::var("MORAINE_TEST_POSTGRES") else {
		return crate::db::sqlite_url(&directory.join("metadata.sqlite"));
	};
	let schema = unique_schema();
	create_schema(&base, &schema).await;
	let separator = if base.contains('?') { '&' } else { '?' };
	let url = format!("{base}{separator}options=-csearch_path%3D{schema}");
	DATABASES
		.lock()
		.expect("databases")
		.push((directory.to_path_buf(), url.clone()));
	url
}

async fn create_schema(base: &str, schema: &str) {
	use sqlx::any::{AnyPoolOptions, install_default_drivers};
	install_default_drivers();
	let pool = AnyPoolOptions::new()
		.max_connections(1)
		.connect(base)
		.await
		.expect("admin pool");
	sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA IF NOT EXISTS {schema}")))
		.execute(&pool)
		.await
		.expect("create schema");
}

fn unique_schema() -> String {
	static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
	let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
	let nanos = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|elapsed| elapsed.as_nanos())
		.unwrap_or(0);
	format!("t{nanos:x}_{count:x}")
}

const TEST_OPERATORS: &[&str] = &[
	"ops@example.org",
	"moderator@example.org",
	"legal@example.org",
	"reviewer@example.org",
	"provider@example.org",
	"operator@example.org",
];

pub(crate) async fn seed_operators(metadata: &MetadataStore) {
	for (index, email) in TEST_OPERATORS.iter().enumerate() {
		if metadata.user_by_email(email).await.ok().flatten().is_some() {
			continue;
		}
		let hash = crate::auth::password::hash_password("correct horse battery").expect("hash");
		metadata
			.create_user(
				&format!("operator-{index}"),
				email,
				&hash,
				crate::auth::OPERATOR_ROLE,
				1_760_000_000,
			)
			.await
			.expect("seed operator");
	}
}

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
		max_mirror_probes_per_cycle: 20,
		max_mirror_probe_bytes: 268_435_456,
		max_feed_page_entries,
		max_feed_scan_pages,
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

pub(crate) async fn body_json(response: Response) -> serde_json::Value {
	let bytes = to_bytes(response.into_body(), 64 * 1024).await.expect("body");
	serde_json::from_slice(&bytes).expect("json")
}

pub(crate) async fn login(application: &Router, email: &str) -> (String, String) {
	let credentials = serde_json::json!({ "email": email, "password": "correct horse battery" }).to_string();
	let register = axum::http::Request::post("/v1/auth/register")
		.header(header::CONTENT_TYPE, "application/json")
		.body(Body::from(credentials.clone()))
		.expect("request");
	application.clone().oneshot(register).await.expect("response");
	let login = axum::http::Request::post("/v1/auth/session")
		.header(header::CONTENT_TYPE, "application/json")
		.body(Body::from(credentials))
		.expect("request");
	let response = application.clone().oneshot(login).await.expect("response");
	let session = set_cookie(&response, "moraine_session");
	let csrf = set_cookie(&response, "moraine_csrf");
	(session, csrf)
}

pub(crate) async fn upload_token(application: &Router, email: &str) -> String {
	scope_token(application, email, "artifacts:write").await
}

pub(crate) async fn scope_token_with_user(application: &Router, email: &str, scope: &str) -> (String, String) {
	let token = scope_token(application, email, scope).await;
	let request = axum::http::Request::get("/v1/auth/me")
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let user_id = body_json(response).await["user_id"].as_str().expect("user id").to_string();
	(token, user_id)
}

pub(crate) async fn scope_token(application: &Router, email: &str, scope: &str) -> String {
	let (session, csrf) = login(application, email).await;
	let cookie = format!("moraine_session={session}; moraine_csrf={csrf}");
	let request = axum::http::Request::post("/v1/auth/keys")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, &cookie)
		.header("x-csrf-token", &csrf)
		.body(Body::from(
			serde_json::json!({ "name": "scoped", "scopes": [scope] }).to_string(),
		))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::CREATED);
	body_json(response).await["key"].as_str().expect("key").to_string()
}

pub(crate) async fn publish_profile(
	application: &Router,
	signer: &SigningKey,
	project_id: &str,
	display_name: &str,
) -> String {
	publish_profile_for_game(application, signer, project_id, display_name, "minecraft").await
}

pub(crate) async fn publish_profile_for_game(
	application: &Router,
	signer: &SigningKey,
	project_id: &str,
	display_name: &str,
	game_id: &str,
) -> String {
	use moraine_model::profile::ProfileRevision;

	let profile = ProfileRevision {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: sample_id(game_id),
		revision_nonce: vec![0x41; 16],
		display_name: display_name.to_string(),
		summary: "A summary".to_string(),
		description: "A description".to_string(),
		icon: None,
		gallery: Vec::new(),
		links: Vec::new(),
		communities: Vec::new(),
		categories: Vec::new(),
		tags: Vec::new(),
		rights: None,
		declared_time: 1_760_000_000,
	};
	let signed = sign_payload(Kind::Profile, &profile, &[signer]);
	let digest = object_id(Kind::Profile, &signed.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/profile"))
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::CREATED);
	let feed = feed_wire_kind(signer, project_id, 1, None, digest, "profile-updated");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(feed))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::CREATED);
	format!("gd:sha256:{}", hex::encode(digest))
}

pub(crate) fn id_bytes(id: &str) -> [u8; 32] {
	let hex = id.strip_prefix("gd:sha256:").expect("object id");
	<[u8; 32]>::try_from(hex::decode(hex).expect("hex").as_slice()).expect("digest")
}

pub(crate) fn set_cookie(response: &Response, name: &str) -> String {
	response
		.headers()
		.get_all(header::SET_COOKIE)
		.iter()
		.find_map(|value| {
			let cookie = value.to_str().ok()?;
			cookie
				.split(';')
				.next()?
				.strip_prefix(&format!("{name}="))
				.map(str::to_string)
		})
		.unwrap_or_default()
}

pub(crate) async fn publish_project(application: &Router, signer: &SigningKey) -> (String, [u8; 32]) {
	publish_project_with_kinds(application, signer, PROJECT_KINDS).await
}

pub(crate) async fn store_release(application: &Router, project_id: &str, wire: Vec<u8>) {
	let path = format!("/v1/projects/{project_id}/objects/release");
	let request = axum::http::Request::post(&path).body(Body::from(wire)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::CREATED);
}

pub(crate) async fn publish_project_with_kinds(
	application: &Router,
	signer: &SigningKey,
	kinds: &[&str],
) -> (String, [u8; 32]) {
	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire(signer, kinds)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let receipt = body_json(response).await;
	let project_id = receipt["project_id"].as_str().expect("project id").to_string();
	let (release, release_digest) = release_wire(signer, &project_id);
	let path = format!("/v1/projects/{project_id}/objects/release");
	let request = axum::http::Request::post(&path).body(Body::from(release)).expect("request");
	application.clone().oneshot(request).await.expect("response");
	(project_id, release_digest)
}
