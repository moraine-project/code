use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{StatusCode, header};
use axum::response::Response;
use moraine_crypto::{ObjectKind as Kind, SigningKey, object_id};
use moraine_model::artifact::Artifact;
use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::release::ReleasePayload;
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use crate::blob::BlobStore;
use crate::capability::Capability;
use crate::db::MetadataStore;
use crate::routes::AppState;

async fn app() -> (Router, tempfile::TempDir) {
	let directory = tempfile::tempdir().expect("tempdir");
	let store = Arc::new(BlobStore::new(directory.path()).await.expect("blob store"));
	let metadata = Arc::new(
		MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("metadata"),
	);
	let config = crate::config::Config {
		bind: "127.0.0.1:0".parse().expect("addr"),
		data_dir: directory.path().to_path_buf(),
		max_artifact_bytes: 1024,
		registration: crate::config::Registration::Open,
		allow_insecure_federation_local: false,
		publishing: crate::config::Publishing::Open,
		..crate::config::Config::default()
	};
	let state = AppState {
		store,
		metadata,
		capability: Arc::new(Capability::discover(&config)),
		branding: std::sync::Arc::new(crate::instance::BrandingSource::default()),
		login_limiter: std::sync::Arc::new(crate::auth::LoginLimiter::new()),
		metrics: std::sync::Arc::new(crate::ops::metrics::Metrics::new()),
		rate_limiter: std::sync::Arc::new(crate::auth::ratelimit::RateLimiter::new()),
		web_dir: None,
	};
	(crate::routes::router(state), directory)
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

fn feed_wire(signer: &SigningKey, project_id: &str, sequence: u64, previous: Option<[u8; 32]>, object: [u8; 32]) -> Vec<u8> {
	let entry = FeedEntry {
		protocol: 1,
		project_id: project_id.to_string(),
		sequence,
		previous: previous.map(|digest| digest.to_vec()),
		kind: "release-published".to_string(),
		object_digest: object.to_vec(),
		declared_at: 1_760_000_000 + sequence as i64,
	};
	sign_payload(Kind::FeedEntry, &entry, &[signer]).wire_bytes()
}

#[tokio::test]
async fn followers_receive_notifications_and_can_read_them() {
	let (application, _directory) = app().await;
	let signer = SigningKey::from_seed(&[31u8; 32]);
	let (cookie, csrf) = login(&application, "fan@example.org").await;

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

	let follow = axum::http::Request::builder()
		.method("POST")
		.uri(format!("/v1/follows/{project_id}"))
		.header(header::COOKIE, &cookie)
		.header("x-csrf-token", &csrf)
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(follow).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(feed_wire(&signer, &project_id, 1, None, release_digest)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let listing = axum::http::Request::get("/v1/notifications?unread=true")
		.header(header::COOKIE, &cookie)
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(listing).await.expect("response");
	let list = body_json(response).await;
	assert_eq!(list.as_array().expect("notifications").len(), 1);
	assert_eq!(list[0]["event_kind"], "release-published");
	let notification_id = list[0]["id"].as_str().expect("id").to_string();

	let read = axum::http::Request::builder()
		.method("POST")
		.uri(format!("/v1/notifications/{notification_id}/read"))
		.header(header::COOKIE, &cookie)
		.header("x-csrf-token", &csrf)
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(read).await.expect("response");
	assert_eq!(response.status(), StatusCode::NO_CONTENT);

	let listing = axum::http::Request::get("/v1/notifications?unread=true")
		.header(header::COOKIE, &cookie)
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(listing).await.expect("response");
	let list = body_json(response).await;
	assert!(list.as_array().expect("notifications").is_empty());
}
#[cfg(test)]
mod prune_tests {
	use crate::db::MetadataStore;
	use crate::federation::notifications::{NotificationRow, now};

	#[tokio::test]
	async fn prunes_notifications_past_retention() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		let old = NotificationRow {
			id: "old".to_string(),
			project_id: "p".to_string(),
			event_kind: "release-published".to_string(),
			object_digest: None,
			feed_seq: Some(1),
			created_at: 1_000,
			read_at: None,
		};
		let fresh = NotificationRow {
			id: "fresh".to_string(),
			created_at: now(),
			..old.clone()
		};
		store.insert_notification(&old, "u").await.expect("old");
		store.insert_notification(&fresh, "u").await.expect("fresh");

		let pruned = store.prune_notifications(now() - 90 * 86_400).await.expect("prune");
		assert_eq!(pruned, 1);
		let remaining = store.notifications("u", false, 10).await.expect("list");
		assert_eq!(remaining.len(), 1);
		assert_eq!(remaining[0].id, "fresh");
	}
}
