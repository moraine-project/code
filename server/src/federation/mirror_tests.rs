use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::get;
use moraine_crypto::{ObjectKind, SigningKey};
use moraine_model::attestation::{AttestationObject, MirrorCommitment};
use moraine_model::signed::sign_payload;
use sha2::Digest;
use tower::ServiceExt;

use crate::blob::BlobStore;
use crate::capability::Capability;
use crate::db::MetadataStore;
use crate::federation::mirrors::*;
use crate::routes::AppState;

async fn app() -> (Router, tempfile::TempDir) {
	let (state, directory) = state().await;
	(crate::routes::router(state), directory)
}

async fn state() -> (AppState, tempfile::TempDir) {
	let directory = tempfile::tempdir().expect("tempdir");
	let store = Arc::new(BlobStore::new(directory.path()).await.expect("blob store"));
	let metadata = Arc::new(
		MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("metadata"),
	);
	crate::test_support::seed_operators(&metadata).await;
	let config = crate::config::Config {
		bind: "127.0.0.1:0".parse().expect("addr"),
		data_dir: directory.path().to_path_buf(),
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
		publishing: crate::config::Publishing::Open,
		web_dir: None,
		s3: Default::default(),
	};
	let state = AppState {
		store,
		metadata,
		capability: Arc::new(Capability::discover(&config)),
		login_limiter: std::sync::Arc::new(crate::auth::LoginLimiter::new()),
		metrics: std::sync::Arc::new(crate::ops::metrics::Metrics::new()),
		rate_limiter: std::sync::Arc::new(crate::auth::ratelimit::RateLimiter::new()),
		web_dir: None,
	};
	(state, directory)
}

async fn login(app: &Router, email: &str) -> (String, String) {
	let credentials = serde_json::json!({ "email": email, "password": "correct horse battery" }).to_string();
	let register = axum::http::Request::post("/v1/auth/register")
		.header(header::CONTENT_TYPE, "application/json")
		.body(Body::from(credentials.clone()))
		.expect("request");
	app.clone().oneshot(register).await.expect("response");
	let session = axum::http::Request::post("/v1/auth/session")
		.header(header::CONTENT_TYPE, "application/json")
		.body(Body::from(credentials))
		.expect("request");
	let response = app.clone().oneshot(session).await.expect("response");
	let token = set_cookie(&response, "moraine_session");
	let csrf = set_cookie(&response, "moraine_csrf");
	(format!("moraine_session={token}; moraine_csrf={csrf}"), csrf)
}

fn set_cookie(response: &Response, name: &str) -> String {
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

async fn body_json(response: Response) -> serde_json::Value {
	let bytes = to_bytes(response.into_body(), 64 * 1024).await.expect("body");
	serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

#[tokio::test]
async fn pins_a_mirror_and_records_a_commitment() {
	let (application, _directory) = app().await;
	let mirror = SigningKey::from_seed(&[21u8; 32]);
	let (cookie, csrf) = login(&application, "ops@example.org").await;

	let pin = axum::http::Request::post("/v1/mirrors/archive-one/keys")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, &cookie)
		.header("x-csrf-token", &csrf)
		.body(Body::from(
			serde_json::json!({ "public_key": hex::encode(mirror.verifying_key().to_bytes()) }).to_string(),
		))
		.expect("request");
	let response = application.clone().oneshot(pin).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let digest = [0x55u8; 32];
	let commitment = MirrorCommitment {
		protocol: 1,
		mirror_id: "archive-one".to_string(),
		artifact_digest: digest.to_vec(),
		size: 4096,
		accepted_at: 1_760_000_400,
		retention_until: Some(1_770_000_000),
		endpoint: "https://mirror.example".to_string(),
	};
	let signed = sign_payload(
		ObjectKind::Attestation,
		&AttestationObject::MirrorCommitment(commitment.clone()),
		&[&mirror],
	);
	let request = axum::http::Request::post("/v1/mirror-commitments")
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let lookup = axum::http::Request::get(format!("/v1/artifacts/sha256/{}/locations", hex::encode(digest)))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(lookup).await.expect("response");
	let view = body_json(response).await;
	assert_eq!(view["commitments"].as_array().expect("commitments").len(), 1);
	assert_eq!(view["commitments"][0]["mirror_id"], "archive-one");

	let unpinned = MirrorCommitment {
		mirror_id: "ghost".to_string(),
		..commitment
	};
	let signed = sign_payload(
		ObjectKind::Attestation,
		&AttestationObject::MirrorCommitment(unpinned),
		&[&mirror],
	);
	let request = axum::http::Request::post("/v1/mirror-commitments")
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn confirms_a_holding_only_when_the_bytes_hash_correctly() {
	let (mut state, _directory) = state().await;
	let capability = crate::capability::Capability {
		allow_insecure_federation_local: true,
		..state.capability.as_ref().clone()
	};
	state.capability = Arc::new(capability);
	let good = b"the real artifact bytes";
	let digest: [u8; 32] = sha2::Sha256::digest(good).into();
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let port = listener.local_addr().expect("addr").port();
	let served = good.to_vec();
	let mock = Router::new().route(
		"/v1/blobs/sha256/{digest}",
		get(move || {
			let served = served.clone();
			async move { served }
		}),
	);
	tokio::spawn(async move {
		let _ = axum::serve(listener, mock).await;
	});

	state
		.metadata
		.insert_commitment(
			&CommitmentRow {
				mirror_id: "archive-one".to_string(),
				size: good.len() as i64,
				accepted_at: now(),
				retention_until: None,
				endpoint: format!("http://127.0.0.1:{port}"),
			},
			&digest,
			&[0x99; 32],
		)
		.await
		.expect("commitment");

	assert_eq!(probe_mirrors(&state, 10).await.expect("probe"), 1);
	let confirmation = state
		.metadata
		.confirmation_for(&digest, "archive-one")
		.await
		.expect("confirmation")
		.expect("recorded");
	assert!(confirmation.reachable);
}

#[tokio::test]
async fn skips_mirror_fetches_larger_than_the_probe_budget() {
	let (mut state, _directory) = state().await;
	let capability = crate::capability::Capability {
		allow_insecure_federation_local: true,
		max_mirror_probe_bytes: 4,
		..state.capability.as_ref().clone()
	};
	state.capability = Arc::new(capability);
	let good = b"more than four bytes";
	let digest: [u8; 32] = sha2::Sha256::digest(good).into();
	state
		.metadata
		.insert_commitment(
			&CommitmentRow {
				mirror_id: "archive-one".to_string(),
				size: good.len() as i64,
				accepted_at: now(),
				retention_until: None,
				endpoint: "http://127.0.0.1:9".to_string(),
			},
			&digest,
			&[0x9a; 32],
		)
		.await
		.expect("commitment");

	assert_eq!(probe_mirrors(&state, 10).await.expect("probe"), 0);
	let confirmation = state
		.metadata
		.confirmation_for(&digest, "archive-one")
		.await
		.expect("confirmation");
	assert!(confirmation.is_none(), "an over-budget commitment is not fetched");
}

#[tokio::test]
async fn locations_round_trip_through_the_store() {
	let directory = tempfile::tempdir().expect("tempdir");
	let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
		.await
		.expect("store");
	store
		.index_location(&[0x11; 32], "https://cdn.example/a.jar", "origin", None, &[0x22; 32])
		.await
		.expect("index");
	let locations = store.locations_for(&[0x11; 32]).await.expect("locations");
	assert_eq!(locations.len(), 1);
	assert_eq!(locations[0].url, "https://cdn.example/a.jar");
	assert_eq!(locations[0].kind, "origin");
}
