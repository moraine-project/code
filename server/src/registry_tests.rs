use std::sync::Arc;

use axum::body::to_bytes;
use moraine_crypto::{ObjectKind as Kind, SigningKey, object_id};
use moraine_model::artifact::Artifact;
use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
use moraine_model::delegation::{Delegation, KeyDelegation};
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::release::ReleasePayload;
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use super::*;
use crate::blob::BlobStore;
use crate::capability::Capability;
use crate::store::MetadataStore;

fn key(byte: u8) -> SigningKey {
	SigningKey::from_seed(&[byte; 32])
}

fn sample_id(label: &str) -> String {
	format!(
		"gd:sha256:{}",
		hex::encode(moraine_crypto::object_id(Kind::Release, label.as_bytes()))
	)
}

async fn app() -> (Router, tempfile::TempDir) {
	app_mode(crate::config::Publishing::Review).await
}

async fn app_mode(publishing: crate::config::Publishing) -> (Router, tempfile::TempDir) {
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
		max_feed_page_entries: 100,
		publishing,
	};
	let state = AppState {
		store,
		metadata,
		capability: Arc::new(Capability::discover(&config)),
	};
	(crate::routes::router(state), directory)
}

const PROJECT_KINDS: &[&str] = &["delegation", "release", "profile"];

fn genesis_wire(signer: &SigningKey, kinds: &[&str]) -> Vec<u8> {
	let genesis = Genesis {
		protocol: 1,
		kind: GenesisKind::Project,
		nonce: vec![0x11; 16],
		roots: vec![RootKey::from_public_key(signer.verifying_key().to_bytes().to_vec()).expect("root")],
		threshold: 1,
		authorized_kinds: kinds.iter().map(|kind| kind.to_string()).collect(),
		home_hint: None,
		contacts: None,
		created_at: 1_760_000_000,
	};
	sign_payload(Kind::Genesis, &genesis, &[signer]).wire_bytes()
}

fn release_wire(signer: &SigningKey, project_id: &str) -> (Vec<u8>, [u8; 32]) {
	let release = ReleasePayload {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: sample_id("minecraft"),
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
	let digest = object_id(Kind::Release, &signed.payload_bytes);
	(signed.wire_bytes(), digest)
}

fn feed_wire(
	signer: &SigningKey,
	project_id: &str,
	sequence: u64,
	previous: Option<[u8; 32]>,
	object_digest: [u8; 32],
) -> Vec<u8> {
	let entry = FeedEntry {
		protocol: 1,
		project_id: project_id.to_string(),
		sequence,
		previous: previous.map(|digest| digest.to_vec()),
		kind: "release-published".to_string(),
		object_digest: object_digest.to_vec(),
		declared_at: 1_760_000_000 + sequence as i64,
	};
	sign_payload(Kind::FeedEntry, &entry, &[signer]).wire_bytes()
}

async fn body_json(response: Response) -> serde_json::Value {
	let bytes = to_bytes(response.into_body(), 64 * 1024).await.expect("body");
	serde_json::from_slice(&bytes).expect("json")
}

#[tokio::test]
async fn publishes_project_object_and_feed_end_to_end() {
	let (application, _directory) = app().await;
	let signer = key(1);

	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire(&signer, PROJECT_KINDS)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let receipt = body_json(response).await;
	let project_id = receipt["project_id"].as_str().expect("project id").to_string();

	let (release, release_digest) = release_wire(&signer, &project_id);
	let path = format!("/v1/projects/{project_id}/objects/release");
	let request = axum::http::Request::post(&path).body(Body::from(release)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let feed = feed_wire(&signer, &project_id, 1, None, release_digest);
	let path = format!("/v1/projects/{project_id}/feed");
	let request = axum::http::Request::post(&path).body(Body::from(feed)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let receipt = body_json(response).await;
	assert_eq!(receipt["seq"], 1);

	let request = axum::http::Request::get(&path).body(Body::empty()).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["entries"].as_array().expect("entries").len(), 1);
	assert_eq!(page["head_seq"], 1);

	let object_path = format!("/v1/objects/{}", hex::encode(release_digest));
	let request = axum::http::Request::get(&object_path).body(Body::empty()).expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn rejects_a_feed_gap() {
	let (application, _directory) = app().await;
	let signer = key(2);
	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire(&signer, PROJECT_KINDS)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let receipt = body_json(response).await;
	let project_id = receipt["project_id"].as_str().expect("project id").to_string();

	let (release, release_digest) = release_wire(&signer, &project_id);
	let path = format!("/v1/projects/{project_id}/objects/release");
	let request = axum::http::Request::post(&path).body(Body::from(release)).expect("request");
	application.clone().oneshot(request).await.expect("response");

	let feed = feed_wire(&signer, &project_id, 2, Some([0u8; 32]), release_digest);
	let path = format!("/v1/projects/{project_id}/feed");
	let request = axum::http::Request::post(&path).body(Body::from(feed)).expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn accepts_a_delegated_release_key_and_rejects_an_unrelated_one() {
	let (application, _directory) = app().await;
	let root = key(1);
	let delegated = key(2);
	let intruder = key(3);
	let kinds = ["delegation", "release", "profile", "feed-entry"];
	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire(&root, &kinds)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let receipt = body_json(response).await;
	let project_id = receipt["project_id"].as_str().expect("project id").to_string();

	let delegation = Delegation::Key(KeyDelegation {
		protocol: 1,
		project_id: project_id.clone(),
		delegate_key: RootKey::from_public_key(delegated.verifying_key().to_bytes().to_vec()).expect("delegate"),
		allowed_kinds: vec!["release".to_string(), "feed-entry".to_string()],
		channels: None,
		max_version_scope: None,
		valid_from_seq: None,
		expires_at: None,
		issued_at: 1_760_000_000,
		previous_delegation_digest: None,
	});
	let wire = sign_payload(Kind::Delegation, &delegation, &[&root]).wire_bytes();
	let path = format!("/v1/projects/{project_id}/objects/delegation");
	let request = axum::http::Request::post(&path).body(Body::from(wire)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let (release, release_digest) = release_wire(&delegated, &project_id);
	let path = format!("/v1/projects/{project_id}/objects/release");
	let request = axum::http::Request::post(&path).body(Body::from(release)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let feed = feed_wire(&delegated, &project_id, 1, None, release_digest);
	let path = format!("/v1/projects/{project_id}/feed");
	let request = axum::http::Request::post(&path).body(Body::from(feed)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let (forged, _) = release_wire(&intruder, &project_id);
	let path = format!("/v1/projects/{project_id}/objects/release");
	let request = axum::http::Request::post(&path).body(Body::from(forged)).expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

async fn login(application: &Router, email: &str) -> (String, String) {
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

async fn publish_project(application: &Router, signer: &SigningKey) -> (String, [u8; 32]) {
	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire(signer, PROJECT_KINDS)))
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

#[tokio::test]
async fn review_mode_queues_then_accepts() {
	let (application, _directory) = app().await;
	let signer = key(1);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let (session, csrf) = login(&application, "reviewer@example.org").await;

	let feed = feed_wire(&signer, &project_id, 1, None, release_digest);
	let submit = axum::http::Request::post("/v1/submissions")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::from(feed))
		.expect("request");
	let response = application.clone().oneshot(submit).await.expect("response");
	assert_eq!(response.status(), StatusCode::ACCEPTED);
	let submission = body_json(response).await;
	let submission_id = submission["id"].as_str().expect("submission id").to_string();
	assert_eq!(submission["state"], "submitted");

	let queue = axum::http::Request::get("/v1/review-queue")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(queue).await.expect("response");
	let queued = body_json(response).await;
	assert_eq!(queued.as_array().expect("queue").len(), 1);

	let review = axum::http::Request::post(format!("/v1/submissions/{submission_id}/review"))
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::from(serde_json::json!({ "decision": "accept" }).to_string()))
		.expect("request");
	let response = application.clone().oneshot(review).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let receipt = body_json(response).await;
	assert_eq!(receipt["state"], "accepted");

	let feed_request = axum::http::Request::get(format!("/v1/projects/{project_id}/feed"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(feed_request).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["head_seq"], 1);
}

#[tokio::test]
async fn open_mode_auto_accepts() {
	let (application, _directory) = app_mode(crate::config::Publishing::Open).await;
	let signer = key(4);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let (session, csrf) = login(&application, "author@example.org").await;

	let feed = feed_wire(&signer, &project_id, 1, None, release_digest);
	let submit = axum::http::Request::post("/v1/submissions")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(feed))
		.expect("request");
	let response = application.clone().oneshot(submit).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let receipt = body_json(response).await;
	assert_eq!(receipt["state"], "auto-accepted");

	let feed_request = axum::http::Request::get(format!("/v1/projects/{project_id}/feed"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(feed_request).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["head_seq"], 1);
}

#[tokio::test]
async fn reject_requires_a_reason_code() {
	let (application, _directory) = app().await;
	let signer = key(5);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let (session, csrf) = login(&application, "reviewer@example.org").await;

	let feed = feed_wire(&signer, &project_id, 1, None, release_digest);
	let submit = axum::http::Request::post("/v1/submissions")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::from(feed))
		.expect("request");
	let submission = body_json(application.clone().oneshot(submit).await.expect("response")).await;
	let submission_id = submission["id"].as_str().expect("submission id").to_string();

	let review = axum::http::Request::post(format!("/v1/submissions/{submission_id}/review"))
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(serde_json::json!({ "decision": "reject" }).to_string()))
		.expect("request");
	let response = application.oneshot(review).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
