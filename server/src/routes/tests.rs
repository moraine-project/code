use axum::body::to_bytes;
use http_body_util::BodyExt;
use tower::ServiceExt;

use super::*;

async fn test_app() -> (Router, tempfile::TempDir) {
	test_app_with(None).await
}

async fn test_app_with_quota(quota: u64) -> (Router, tempfile::TempDir) {
	let (mut state, directory) = test_state(None).await;
	let capability = crate::capability::Capability {
		max_upload_bytes_per_account: quota,
		max_projects: 10_000,
		max_mirror_probes_per_cycle: 20,
		max_mirror_probe_bytes: 268_435_456,
		..state.capability.as_ref().clone()
	};
	state.capability = Arc::new(capability);
	(router(state), directory)
}

async fn test_app_with_origins(origins: Vec<String>) -> (Router, tempfile::TempDir) {
	let (mut state, directory) = test_state(None).await;
	let capability = crate::capability::Capability {
		web_origins: origins,
		..state.capability.as_ref().clone()
	};
	state.capability = Arc::new(capability);
	(router(state), directory)
}

async fn test_state(web_dir: Option<std::path::PathBuf>) -> (AppState, tempfile::TempDir) {
	let directory = tempfile::tempdir().expect("tempdir");
	let store = BlobStore::new(directory.path()).await.expect("store");
	let state = state_with(store, directory.path(), web_dir).await;
	(state, directory)
}

async fn state_with(store: BlobStore, directory: &std::path::Path, web_dir: Option<std::path::PathBuf>) -> AppState {
	let store = Arc::new(store);
	let metadata = Arc::new(
		MetadataStore::open(directory.join("metadata.sqlite"))
			.await
			.expect("metadata"),
	);
	let config = crate::config::Config {
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
		max_mirror_probes_per_cycle: 20,
		max_mirror_probe_bytes: 268_435_456,
		max_feed_page_entries: 100,
		max_response_bytes: 16_777_216,
		staging_retention_seconds: 3_600,
		blob_retention_seconds: 604_800,
		max_sync_pages: 200,
		requests_per_minute: 600,
		max_concurrent_syncs: 4,
		maintenance_interval_seconds: 3_600,
		tls_extra_roots: None,
		max_feed_scan_pages: 50,
		skip_migrate_on_start: false,
		database_url: None,
		allow_insecure_federation_local: false,
		publishing: crate::config::Publishing::Review,
		web_dir: web_dir.clone(),
		s3: Default::default(),
	};
	AppState {
		store,
		metadata,
		capability: Arc::new(Capability::discover(&config)),
		login_limiter: std::sync::Arc::new(crate::auth::LoginLimiter::new()),
		metrics: std::sync::Arc::new(crate::ops::metrics::Metrics::new()),
		rate_limiter: std::sync::Arc::new(crate::auth::ratelimit::RateLimiter::new()),
		web_dir: web_dir.map(Arc::new),
	}
}

async fn test_app_with(web_dir: Option<std::path::PathBuf>) -> (Router, tempfile::TempDir) {
	let (state, directory) = test_state(web_dir).await;
	(router(state), directory)
}

#[tokio::test]
async fn serves_capability_and_health() {
	let (app, _directory) = test_app().await;
	let response = app
		.clone()
		.oneshot(
			axum::http::Request::get("/.well-known/mod-registry")
				.body(Body::empty())
				.expect("request"),
		)
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let body = to_bytes(response.into_body(), 16 * 1024).await.expect("body");
	let capability: serde_json::Value = serde_json::from_slice(&body).expect("json");
	assert_eq!(capability["protocol_versions"][0], 1);

	let health = app
		.oneshot(axum::http::Request::get("/healthz").body(Body::empty()).expect("request"))
		.await
		.expect("response");
	assert_eq!(health.status(), StatusCode::OK);
}

#[tokio::test]
async fn uploads_then_serves_round_trip() {
	let (app, directory) = test_app().await;
	let token = crate::test_support::upload_token(&app, "uploader@example.org").await;
	let anonymous = axum::http::Request::post("/v1/blobs")
		.body(Body::from("artifact"))
		.expect("request");
	let response = app.clone().oneshot(anonymous).await.expect("response");
	assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

	let upload = axum::http::Request::post("/v1/blobs")
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::from("artifact"))
		.expect("request");
	let response = app.clone().oneshot(upload).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let body = to_bytes(response.into_body(), 16 * 1024).await.expect("body");
	let receipt: serde_json::Value = serde_json::from_slice(&body).expect("json");
	let digest = receipt["digest"].as_str().expect("digest");
	let hex_digest = digest.strip_prefix("sha256:").expect("prefix");
	let path = format!("/v1/blobs/sha256/{hex_digest}");

	let draft = axum::http::Request::get(&path).body(Body::empty()).expect("request");
	let response = app.clone().oneshot(draft).await.expect("response");
	assert_eq!(response.status(), StatusCode::NOT_FOUND);

	let metadata = crate::db::MetadataStore::open(directory.path().join("metadata.sqlite"))
		.await
		.expect("metadata");
	metadata
		.index_artifact(hex::decode(hex_digest).expect("hex").as_slice(), "p", &[0u8; 32])
		.await
		.expect("index");

	let served = axum::http::Request::get(&path).body(Body::empty()).expect("request");
	let response = app.oneshot(served).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let body = response.into_body().collect().await.expect("collect").to_bytes();
	assert_eq!(body.as_ref(), b"artifact");
}

fn live_s3_settings() -> Option<(String, crate::config::S3Settings)> {
	let bucket = std::env::var("MORAINE_TEST_S3_BUCKET").ok()?;
	Some((
		bucket.clone(),
		crate::config::S3Settings {
			bucket: Some(bucket),
			endpoint: std::env::var("MORAINE_TEST_S3_ENDPOINT").ok(),
			region: std::env::var("MORAINE_TEST_S3_REGION").ok(),
			access_key_id: std::env::var("MORAINE_TEST_S3_ACCESS_KEY_ID").ok(),
			secret_access_key: std::env::var("MORAINE_TEST_S3_SECRET_ACCESS_KEY").ok(),
			prefix: format!(
				"moraine-test-{}",
				std::time::SystemTime::now()
					.duration_since(std::time::UNIX_EPOCH)
					.map(|duration| duration.as_nanos())
					.unwrap_or(0)
			),
		},
	))
}

#[tokio::test]
async fn uploads_and_serves_through_object_storage() {
	let Some((bucket, settings)) = live_s3_settings() else {
		return;
	};
	let directory = tempfile::tempdir().expect("tempdir");
	let store = BlobStore::with_object_store(
		crate::blob::s3_object_store(&bucket, &settings).expect("s3"),
		settings.prefix.clone(),
		directory.path().join("staging"),
	)
	.await
	.expect("store");
	let state = state_with(store, directory.path(), None).await;
	let app = router(state);

	let token = crate::test_support::upload_token(&app, "uploader@example.org").await;
	let upload = axum::http::Request::post("/v1/blobs")
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::from("object artifact"))
		.expect("request");
	let response = app.clone().oneshot(upload).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let receipt: serde_json::Value =
		serde_json::from_slice(&to_bytes(response.into_body(), 16 * 1024).await.expect("body")).expect("json");
	let hex_digest = receipt["digest"]
		.as_str()
		.expect("digest")
		.strip_prefix("sha256:")
		.expect("prefix")
		.to_string();
	let path = format!("/v1/blobs/sha256/{hex_digest}");

	crate::db::MetadataStore::open(directory.path().join("metadata.sqlite"))
		.await
		.expect("metadata")
		.index_artifact(hex::decode(&hex_digest).expect("hex").as_slice(), "p", &[0u8; 32])
		.await
		.expect("index");

	let served = axum::http::Request::get(&path).body(Body::empty()).expect("request");
	let response = app.clone().oneshot(served).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let body = response.into_body().collect().await.expect("collect").to_bytes();
	assert_eq!(body.as_ref(), b"object artifact");

	let ranged = axum::http::Request::get(&path)
		.header(header::RANGE, "bytes=7-14")
		.body(Body::empty())
		.expect("request");
	let response = app.oneshot(ranged).await.expect("response");
	assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
	let body = response.into_body().collect().await.expect("collect").to_bytes();
	assert_eq!(body.as_ref(), b"artifact");
}

#[tokio::test]
async fn refuses_an_upload_past_the_account_quota() {
	let (app, _directory) = test_app_with_quota(10).await;
	let token = crate::test_support::upload_token(&app, "quota@example.org").await;
	let upload = |body: &'static str| {
		axum::http::Request::post("/v1/blobs")
			.header(header::AUTHORIZATION, format!("Bearer {token}"))
			.body(Body::from(body))
			.expect("request")
	};

	let response = app.clone().oneshot(upload("0123456789")).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let response = app.oneshot(upload("abcdefghij")).await.expect("response");
	assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn serves_blob_with_ranges() {
	let (app, directory) = test_app().await;
	let store = BlobStore::new(directory.path()).await.expect("store");
	let staged = store.put_staged(b"0123456789".as_slice(), 1024).await.expect("stage");
	let digest = store.commit(staged).await.expect("commit");
	crate::db::MetadataStore::open(directory.path().join("metadata.sqlite"))
		.await
		.expect("metadata")
		.index_artifact(&digest, "p", &[0u8; 32])
		.await
		.expect("index");
	let path = format!("/v1/blobs/sha256/{}", hex::encode(digest));

	let full = app
		.clone()
		.oneshot(axum::http::Request::get(&path).body(Body::empty()).expect("request"))
		.await
		.expect("response");
	assert_eq!(full.status(), StatusCode::OK);
	let body = full.into_body().collect().await.expect("collect").to_bytes();
	assert_eq!(body.as_ref(), b"0123456789");

	let range = axum::http::Request::get(&path)
		.header(header::RANGE, "bytes=2-5")
		.body(Body::empty())
		.expect("request");
	let partial = app.clone().oneshot(range).await.expect("response");
	assert_eq!(partial.status(), StatusCode::PARTIAL_CONTENT);
	assert_eq!(partial.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
	let body = partial.into_body().collect().await.expect("collect").to_bytes();
	assert_eq!(body.as_ref(), b"2345");

	let missing =
		axum::http::Request::get("/v1/blobs/sha256/0000000000000000000000000000000000000000000000000000000000000000")
			.body(Body::empty())
			.expect("request");
	let response = app.oneshot(missing).await.expect("response");
	assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn reads_are_cors_enabled_but_writes_are_not_offered() {
	let (app, _directory) = test_app().await;
	let read = axum::http::Request::get("/.well-known/mod-registry")
		.header(header::ORIGIN, "https://site.example")
		.body(Body::empty())
		.expect("request");
	let response = app.clone().oneshot(read).await.expect("response");
	assert_eq!(response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");

	let preflight = axum::http::Request::builder()
		.method("OPTIONS")
		.uri("/v1/submissions")
		.header(header::ORIGIN, "https://site.example")
		.header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
		.body(Body::empty())
		.expect("request");
	let response = app.oneshot(preflight).await.expect("response");
	let methods = response
		.headers()
		.get(header::ACCESS_CONTROL_ALLOW_METHODS)
		.and_then(|value| value.to_str().ok())
		.unwrap_or_default()
		.to_ascii_uppercase();
	assert!(!methods.contains("POST"));
}

#[tokio::test]
async fn an_allowlisted_origin_may_write_with_credentials() {
	let (app, _directory) = test_app_with_origins(vec!["https://site.example".to_string()]).await;
	let preflight = axum::http::Request::builder()
		.method("OPTIONS")
		.uri("/v1/blobs")
		.header(header::ORIGIN, "https://site.example")
		.header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
		.body(Body::empty())
		.expect("request");
	let response = app.clone().oneshot(preflight).await.expect("response");
	assert_eq!(
		response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
		"https://site.example"
	);
	assert_eq!(response.headers()[header::ACCESS_CONTROL_ALLOW_CREDENTIALS], "true");
	let methods = response
		.headers()
		.get(header::ACCESS_CONTROL_ALLOW_METHODS)
		.and_then(|value| value.to_str().ok())
		.unwrap_or_default()
		.to_ascii_uppercase();
	assert!(methods.contains("POST"), "{methods}");

	let other = axum::http::Request::builder()
		.method("OPTIONS")
		.uri("/v1/blobs")
		.header(header::ORIGIN, "https://evil.example")
		.header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
		.body(Body::empty())
		.expect("request");
	let response = app.oneshot(other).await.expect("response");
	assert!(response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}

#[tokio::test]
async fn serves_a_built_site_with_spa_fallback() {
	let site = tempfile::tempdir().expect("site");
	std::fs::write(site.path().join("index.html"), "<!doctype html><title>Moraine App</title>").expect("index");
	std::fs::write(site.path().join("asset.txt"), "asset").expect("asset");
	let (app, _directory) = test_app_with(Some(site.path().to_path_buf())).await;

	let root = axum::http::Request::get("/").body(Body::empty()).expect("request");
	let response = app.clone().oneshot(root).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let body = to_bytes(response.into_body(), 16 * 1024).await.expect("body");
	assert!(String::from_utf8_lossy(&body).contains("Moraine App"));

	let asset = axum::http::Request::get("/asset.txt").body(Body::empty()).expect("request");
	let response = app.clone().oneshot(asset).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let spa_route = axum::http::Request::get("/p/gd:sha256:deadbeef")
		.body(Body::empty())
		.expect("request");
	let response = app.clone().oneshot(spa_route).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let api_miss = axum::http::Request::get("/v1/projects/nope")
		.body(Body::empty())
		.expect("request");
	let response = app.oneshot(api_miss).await.expect("response");
	assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn responses_carry_security_headers() {
	let (app, _directory) = test_app().await;
	let request = axum::http::Request::get("/healthz").body(Body::empty()).expect("request");
	let response = app.oneshot(request).await.expect("response");
	assert_eq!(
		response.headers().get(header::X_CONTENT_TYPE_OPTIONS).expect("nosniff"),
		"nosniff"
	);
	assert_eq!(
		response.headers().get(header::REFERRER_POLICY).expect("referrer"),
		"no-referrer"
	);
	let policy = response
		.headers()
		.get(header::CONTENT_SECURITY_POLICY)
		.expect("csp")
		.to_str()
		.expect("csp");
	assert!(policy.contains("frame-ancestors 'none'"));
}
