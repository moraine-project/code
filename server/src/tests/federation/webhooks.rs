use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::post as route_post;
use moraine_crypto::{ALG_ED25519, ObjectKind as Kind, SigningKey, VerifyingKey, object_id};
use moraine_model::artifact::Artifact;
use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::release::ReleasePayload;
use moraine_model::signed::sign_payload;
use tokio::net::TcpListener;
use tower::ServiceExt;

use crate::blob::BlobStore;
use crate::capability::Capability;
use crate::db::MetadataStore;
use crate::federation::webhooks::deliver_pending;
use crate::routes::AppState;

async fn app() -> (Router, AppState, tempfile::TempDir) {
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
		scanner_enabled: false,
		scanner_provider_id: "local-clamav".to_string(),
		scanner_kind: "clamav".to_string(),
		scanner_command: "clamscan".to_string(),
		scanner_args: Vec::new(),
		scanner_timeout_seconds: 300,
		skip_migrate_on_start: false,
		database_url: None,
		allow_insecure_federation_local: true,
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
	(crate::routes::router(state.clone()), state, directory)
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

fn genesis_wire(signer: &SigningKey) -> Vec<u8> {
	let genesis = Genesis {
		protocol: 1,
		kind: GenesisKind::Project,
		nonce: vec![0x11; 16],
		roots: vec![RootKey::from_public_key(signer.verifying_key().to_bytes().to_vec()).expect("root")],
		threshold: 1,
		authorized_kinds: vec!["delegation".to_string(), "release".to_string(), "profile".to_string()],
		home_hint: None,
		contacts: None,
		created_at: 1_760_000_000,
	};
	sign_payload(Kind::Genesis, &genesis, &[signer]).wire_bytes()
}

fn release_wire(signer: &SigningKey, project_id: &str) -> ([u8; 32], Vec<u8>) {
	let release = ReleasePayload {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: "gd:sha256:game".to_string(),
		release_nonce: vec![0x42; 16],
		human_version: "1.0.0".to_string(),
		channel: "release".to_string(),
		kind: "mod".to_string(),
		declared_time: 1_760_000_000,
		compatibility: vec![Compatibility {
			game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
			loader_id: None,
			loader_version_predicate: None,
			side: Side::Both,
			runtime_predicate: None,
			os_predicate: None,
			arch_predicate: None,
		}],
		artifacts: vec![Artifact {
			digest: vec![0xAB; 32],
			size: 10,
			media_type: "application/java-archive".to_string(),
			filename: "example.jar".to_string(),
			is_primary: true,
			os_predicate: None,
			arch_predicate: None,
		}],
		dependencies: Vec::new(),
		source_reference: None,
		changelog_digest: None,
		license_expression: None,
		rights: None,
		sbom_digest: None,
		minimum_verifier_version: 1,
		critical_extensions: Vec::new(),
	};
	let signed = sign_payload(Kind::Release, &release, &[signer]);
	(object_id(Kind::Release, &signed.payload_bytes), signed.wire_bytes())
}

fn feed_wire(signer: &SigningKey, project_id: &str, sequence: u64, object: [u8; 32]) -> Vec<u8> {
	let entry = FeedEntry {
		protocol: 1,
		project_id: project_id.to_string(),
		sequence,
		previous: None,
		kind: "release-published".to_string(),
		object_digest: object.to_vec(),
		declared_at: 1_760_000_000 + sequence as i64,
	};
	sign_payload(Kind::FeedEntry, &entry, &[signer]).wire_bytes()
}

#[tokio::test]
async fn delivers_a_signed_event_and_verifies_it() {
	let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
	let sink = {
		let received = received.clone();
		Router::new().route(
			"/sink",
			route_post(move |body: Bytes| {
				let received = received.clone();
				async move {
					let value: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
					received.lock().expect("lock").push(value);
					StatusCode::NO_CONTENT
				}
			}),
		)
	};
	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	tokio::spawn(async move {
		let _ = axum::serve(listener, sink).await;
	});

	let (application, state, _directory) = app().await;
	let signer = SigningKey::from_seed(&[41u8; 32]);
	let (cookie, csrf) = login(&application, "operator@example.org").await;

	let create = axum::http::Request::builder()
		.method("POST")
		.uri("/v1/webhooks")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, &cookie)
		.header("x-csrf-token", &csrf)
		.body(Body::from(
			serde_json::json!({
				"url": format!("http://127.0.0.1:{}/sink", address.port()),
				"event_kinds": ["release-published"],
			})
			.to_string(),
		))
		.expect("request");
	let response = application.clone().oneshot(create).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire(&signer)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let project_id = body_json(response).await["project_id"]
		.as_str()
		.expect("project id")
		.to_string();

	let (release_digest, release) = release_wire(&signer, &project_id);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(release))
		.expect("request");
	application.clone().oneshot(request).await.expect("response");

	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(feed_wire(&signer, &project_id, 1, release_digest)))
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let delivered = deliver_pending(&state, 10).await.expect("deliver");
	assert_eq!(delivered, 1);

	let bodies = received.lock().expect("lock").clone();
	assert_eq!(bodies.len(), 1);
	let payload = hex::decode(bodies[0]["payload"].as_str().expect("payload")).expect("hex");
	let signature = hex::decode(bodies[0]["signature"].as_str().expect("signature")).expect("hex");
	let public_key = state.capability.webhook_public_key.as_ref().expect("public key");
	let key = VerifyingKey::from_bytes(ALG_ED25519, &hex::decode(public_key).expect("hex")).expect("key");
	let mut message = moraine_model::event::WEBHOOK_DOMAIN.to_vec();
	message.extend_from_slice(&payload);
	assert!(key.verify(&message, &signature).is_ok());

	let again = deliver_pending(&state, 10).await.expect("deliver");
	assert_eq!(again, 0);
}
#[cfg(test)]
mod prune_tests {
	use crate::db::MetadataStore;
	use crate::federation::webhooks::now;

	#[tokio::test]
	async fn prunes_delivery_records_past_retention() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		store
			.enqueue_delivery("old", "wh", "e1", "https://mirror.example", "{}", 1_000)
			.await
			.expect("old");
		store
			.enqueue_delivery("fresh", "wh", "e2", "https://mirror.example", "{}", now())
			.await
			.expect("fresh");

		let pruned = store.prune_deliveries(now() - 30 * 86_400).await.expect("prune");
		assert_eq!(pruned, 1);
	}
}
