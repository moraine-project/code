use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::header;
use moraine_crypto::{ObjectKind as Kind, SigningKey, object_id};
use moraine_model::advisory::{Advisory, Affected, Category, Severity};
use moraine_model::artifact::Artifact;
use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
use moraine_model::definition::{GameDef, VersionSyntax};
use moraine_model::delegation::{Delegation, KeyDelegation, OwnerRef, OwnershipTransfer};
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::release::{ReleasePayload, Withdrawal};
use moraine_model::signed::sign_payload;
use tokio::net::TcpListener;
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
	app_mode(crate::config::Publishing::Open, false).await
}

async fn app_review() -> (Router, tempfile::TempDir) {
	app_mode(crate::config::Publishing::Review, false).await
}

async fn app_mode(
	publishing: crate::config::Publishing,
	allow_insecure_federation_local: bool,
) -> (Router, tempfile::TempDir) {
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
		allow_insecure_federation_local,
		publishing,
		web_dir: None,
	};
	let state = AppState {
		store,
		metadata,
		capability: Arc::new(Capability::discover(&config)),
		web_dir: None,
	};
	(crate::routes::router(state), directory)
}

const PROJECT_KINDS: &[&str] = &["delegation", "release", "profile"];

fn game_genesis_wire(key: &SigningKey) -> Vec<u8> {
	let genesis = Genesis {
		protocol: 1,
		kind: GenesisKind::Game,
		nonce: vec![0x21; 16],
		roots: vec![RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).expect("root")],
		threshold: 1,
		authorized_kinds: vec!["delegation".to_string(), "game-def".to_string()],
		home_hint: None,
		contacts: None,
		created_at: 1_760_000_000,
	};
	sign_payload(Kind::Genesis, &genesis, &[key]).wire_bytes()
}

fn game_definition_wire(key: &SigningKey, game_id: &str) -> Vec<u8> {
	let definition = GameDef {
		protocol: 1,
		game_id: game_id.to_string(),
		display_name: "Minecraft".to_string(),
		version_syntax: VersionSyntax {
			kind: "semver".to_string(),
			pattern: None,
		},
		version_ordering: "semver".to_string(),
		loaders_allowed: true,
		loader_authorities: Vec::new(),
		categories: Vec::new(),
		tags: Vec::new(),
		metadata_extractor: None,
		install_adapter: None,
		declared_time: 1_760_000_000,
	};
	sign_payload(Kind::GameDef, &definition, &[key]).wire_bytes()
}

fn genesis_wire(signer: &SigningKey, kinds: &[&str]) -> Vec<u8> {
	genesis_wire_roots(&[signer], kinds)
}

fn genesis_wire_roots(roots: &[&SigningKey], kinds: &[&str]) -> Vec<u8> {
	let genesis = Genesis {
		protocol: 1,
		kind: GenesisKind::Project,
		nonce: vec![0x11; 16],
		roots: roots
			.iter()
			.map(|signer| RootKey::from_public_key(signer.verifying_key().to_bytes().to_vec()).expect("root"))
			.collect(),
		threshold: 1,
		authorized_kinds: kinds.iter().map(|kind| kind.to_string()).collect(),
		home_hint: None,
		contacts: None,
		created_at: 1_760_000_000,
	};
	sign_payload(Kind::Genesis, &genesis, roots.to_vec().as_slice()).wire_bytes()
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
	feed_wire_kind(signer, project_id, sequence, previous, object_digest, "release-published")
}

fn feed_wire_kind(
	signer: &SigningKey,
	project_id: &str,
	sequence: u64,
	previous: Option<[u8; 32]>,
	object_digest: [u8; 32],
	kind: &str,
) -> Vec<u8> {
	let entry = FeedEntry {
		protocol: 1,
		project_id: project_id.to_string(),
		sequence,
		previous: previous.map(|digest| digest.to_vec()),
		kind: kind.to_string(),
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
	assert_eq!(page["entries"][0]["title"], "1.0.0 (release)");

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
	let (application, _directory) = app_review().await;
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
	let (application, _directory) = app_mode(crate::config::Publishing::Open, false).await;
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
	let (application, _directory) = app_review().await;
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

#[tokio::test]
async fn federation_syncs_a_home_feed() {
	let home_signer = key(6);
	let (home, _home_directory) = app().await;
	let (project_id, release_digest) = publish_project(&home, &home_signer).await;
	let feed = feed_wire(&home_signer, &project_id, 1, None, release_digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(feed))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	let serving = home.clone();
	tokio::spawn(async move {
		let _ = axum::serve(listener, serving).await;
	});

	let (directory, _directory_dir) = app_mode(crate::config::Publishing::Review, true).await;
	let (session, csrf) = login(&directory, "ops@example.org").await;
	let sync = axum::http::Request::post("/v1/federation/sync")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(
			serde_json::json!({
				"home_url": format!("http://127.0.0.1:{}", address.port()),
				"project_id": project_id,
			})
			.to_string(),
		))
		.expect("request");
	let response = directory.clone().oneshot(sync).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let report = body_json(response).await;
	assert_eq!(report["applied"], 1);

	let feed_request = axum::http::Request::get(format!("/v1/projects/{project_id}/feed"))
		.body(Body::empty())
		.expect("request");
	let response = directory.clone().oneshot(feed_request).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["head_seq"], 1);

	let subscriptions = axum::http::Request::get("/v1/subscriptions")
		.header(header::COOKIE, format!("moraine_session={session}"))
		.body(Body::empty())
		.expect("request");
	let response = directory.oneshot(subscriptions).await.expect("response");
	let list = body_json(response).await;
	assert_eq!(list.as_array().expect("subscriptions").len(), 1);
	assert_eq!(list[0]["cursor_seq"], 1);
}

#[tokio::test]
async fn serves_json_views_of_profile_and_release() {
	use moraine_model::profile::ProfileRevision;

	let (application, _directory) = app().await;
	let signer = key(7);
	let (project_id, release_digest) = publish_project(&application, &signer).await;

	let profile = ProfileRevision {
		protocol: 1,
		project_id: project_id.clone(),
		game_id: sample_id("minecraft"),
		revision_nonce: vec![0x24; 16],
		display_name: "Example Mod".to_string(),
		summary: "A worked example".to_string(),
		description: "Longer description".to_string(),
		icon: None,
		gallery: Vec::new(),
		links: Vec::new(),
		communities: Vec::new(),
		categories: vec!["utility".to_string()],
		tags: vec!["client".to_string()],
		rights: None,
		declared_time: 1_760_000_000,
	};
	let signed_profile = sign_payload(Kind::Profile, &profile, &[&signer]);
	let profile_digest = object_id(Kind::Profile, &signed_profile.payload_bytes);
	let path = format!("/v1/projects/{project_id}/objects/profile");
	let request = axum::http::Request::post(&path)
		.body(Body::from(signed_profile.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let entry = FeedEntry {
		protocol: 1,
		project_id: project_id.clone(),
		sequence: 1,
		previous: None,
		kind: "profile-updated".to_string(),
		object_digest: profile_digest.to_vec(),
		declared_at: 1_760_000_000,
	};
	let feed = sign_payload(Kind::FeedEntry, &entry, &[&signer]).wire_bytes();
	let path = format!("/v1/projects/{project_id}/feed");
	let request = axum::http::Request::post(&path).body(Body::from(feed)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let profile_request = axum::http::Request::get(format!("/v1/projects/{project_id}/profile"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(profile_request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["display_name"], "Example Mod");
	assert_eq!(view["tags"][0], "client");

	let release_request =
		axum::http::Request::get(format!("/v1/projects/{project_id}/releases/{}", hex::encode(release_digest)))
			.body(Body::empty())
			.expect("request");
	let response = application.clone().oneshot(release_request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["human_version"], "1.0.0");
	assert_eq!(view["artifacts"][0]["is_primary"], true);

	let search = axum::http::Request::get("/v1/search?q=example")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(search).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let page = body_json(response).await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);
	assert_eq!(page["results"][0]["display_name"], "Example Mod");

	let tagged = axum::http::Request::get("/v1/search?tag=client")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(tagged).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);

	let missing = axum::http::Request::get("/v1/search?tag=server")
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(missing).await.expect("response");
	let page = body_json(response).await;
	assert!(page["results"].as_array().expect("results").is_empty());
}

#[tokio::test]
async fn looks_up_a_release_from_an_artifact_digest() {
	let (application, _directory) = app().await;
	let signer = key(8);
	let (project_id, release_digest) = publish_project(&application, &signer).await;

	let digest = hex::encode([0xABu8; 32]);
	let request = axum::http::Request::get(format!("/v1/lookup?sha256={digest}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["matches"].as_array().expect("matches").len(), 1);
	assert_eq!(view["matches"][0]["project_id"], project_id);
	assert_eq!(
		view["matches"][0]["release"],
		format!("gd:sha256:{}", hex::encode(release_digest))
	);
	assert_eq!(view["matches"][0]["human_version"], "1.0.0");
	assert_eq!(view["matches"][0]["filename"], "example.jar");

	let unknown = axum::http::Request::get(format!("/v1/lookup?sha256={}", hex::encode([9u8; 32])))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(unknown).await.expect("response");
	let view = body_json(response).await;
	assert!(view["matches"].as_array().expect("matches").is_empty());
}

#[tokio::test]
async fn review_mode_refuses_direct_feed_append() {
	let (application, _directory) = app_review().await;
	let signer = key(9);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let feed = feed_wire(&signer, &project_id, 1, None, release_digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(feed))
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn ownership_transfer_requires_two_signatures_and_updates_the_owner() {
	let (application, _directory) = app().await;
	let first = key(10);
	let second = key(11);
	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire_roots(&[&first, &second], PROJECT_KINDS)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let receipt = body_json(response).await;
	let project_id = receipt["project_id"].as_str().expect("project id").to_string();

	let transfer = Delegation::OwnershipTransfer(OwnershipTransfer {
		protocol: 1,
		project_id: project_id.clone(),
		from_owner: OwnerRef {
			kind: "user".to_string(),
			id: "user-a".to_string(),
		},
		to_owner: OwnerRef {
			kind: "org".to_string(),
			id: "org-b".to_string(),
		},
		issued_at: 1_760_000_000,
		previous_delegation_digest: None,
	});
	let signed_transfer = sign_payload(Kind::Delegation, &transfer, &[&first, &second]);
	let transfer_digest = object_id(Kind::Delegation, &signed_transfer.payload_bytes);
	let response = application
		.clone()
		.oneshot(
			axum::http::Request::post(format!("/v1/projects/{project_id}/transfer"))
				.body(Body::from(signed_transfer.wire_bytes()))
				.expect("request"),
		)
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let entry = feed_wire_kind(&first, &project_id, 1, None, transfer_digest, "ownership-transferred");
	let response = application
		.clone()
		.oneshot(
			axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
				.body(Body::from(entry))
				.expect("request"),
		)
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let summary = body_json(
		application
			.clone()
			.oneshot(
				axum::http::Request::get(format!("/v1/projects/{project_id}"))
					.body(Body::empty())
					.expect("request"),
			)
			.await
			.expect("response"),
	)
	.await;
	assert_eq!(summary["owner"]["id"], "org-b");

	let one_signature = Delegation::OwnershipTransfer(OwnershipTransfer {
		protocol: 1,
		project_id: project_id.clone(),
		from_owner: OwnerRef {
			kind: "org".to_string(),
			id: "org-b".to_string(),
		},
		to_owner: OwnerRef {
			kind: "user".to_string(),
			id: "user-c".to_string(),
		},
		issued_at: 1_760_000_001,
		previous_delegation_digest: None,
	});
	let wire = sign_payload(Kind::Delegation, &one_signature, &[&first]).wire_bytes();
	let response = application
		.oneshot(
			axum::http::Request::post(format!("/v1/projects/{project_id}/transfer"))
				.body(Body::from(wire))
				.expect("request"),
		)
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn withdrawal_marks_a_release_without_rewriting_it() {
	let (application, _directory) = app().await;
	let signer = key(12);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let release_id = format!("gd:sha256:{}", hex::encode(release_digest));

	let withdrawal = Withdrawal {
		protocol: 1,
		release_id: release_id.clone(),
		reason: "compromise".to_string(),
		note: Some("automated key leak".to_string()),
		declared_time: 1_760_000_100,
	};
	let signed = sign_payload(Kind::Release, &withdrawal, &[&signer]);
	let digest = object_id(Kind::Release, &signed.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let entry = feed_wire_kind(&signer, &project_id, 1, None, digest, "release-withdrawn");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let request = axum::http::Request::get(format!("/v1/projects/{project_id}/releases/{}", hex::encode(release_digest)))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	let view = body_json(response).await;
	assert_eq!(view["withdrawal"]["reason"], "compromise");
	assert_eq!(view["withdrawal"]["note"], "automated key leak");
	assert_eq!(view["human_version"], "1.0.0");
}

#[tokio::test]
async fn feed_titles_a_withdrawal() {
	let (application, _directory) = app().await;
	let signer = key(13);
	let (project_id, _release_digest) = publish_project(&application, &signer).await;

	let withdrawal = Withdrawal {
		protocol: 1,
		release_id: "gd:sha256:whatever".to_string(),
		reason: "compromise".to_string(),
		note: None,
		declared_time: 1_760_000_200,
	};
	let signed = sign_payload(Kind::Release, &withdrawal, &[&signer]);
	let digest = object_id(Kind::Release, &signed.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	application.clone().oneshot(request).await.expect("response");

	let entry = feed_wire_kind(&signer, &project_id, 1, None, digest, "release-withdrawn");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	application.clone().oneshot(request).await.expect("response");

	let request = axum::http::Request::get(format!("/v1/projects/{project_id}/feed"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["entries"][0]["title"], "withdrawn: compromise");
}

#[tokio::test]
async fn release_view_lists_pinned_provider_advisories() {
	let (application, _directory) = app().await;
	let signer = key(14);
	let provider = key(15);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let (session, csrf) = login(&application, "provider@example.org").await;
	let cookie = format!("moraine_session={session}; moraine_csrf={csrf}");

	let pin = axum::http::Request::post("/v1/providers/scanner/keys")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, &cookie)
		.header("x-csrf-token", &csrf)
		.body(Body::from(
			serde_json::json!({ "public_key": hex::encode(provider.verifying_key().to_bytes()) }).to_string(),
		))
		.expect("request");
	let response = application.clone().oneshot(pin).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let advisory = Advisory {
		protocol: 1,
		provider_id: "scanner".to_string(),
		project_id: project_id.clone(),
		game_id: sample_id("minecraft"),
		affected: Affected {
			digest: Some(vec![0xAB; 32]),
			predicate: None,
		},
		severity: Severity::High,
		category: Category::Malware,
		taxonomy_version: 1,
		block_promotion: true,
		evidence_ref: None,
		published_at: 1_760_000_300,
		expires_at: None,
		retracted_at: None,
	};
	let signed = sign_payload(Kind::Advisory, &advisory, &[&provider]);
	let request = axum::http::Request::post("/v1/advisories")
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let request = axum::http::Request::get(format!("/v1/projects/{project_id}/releases/{}", hex::encode(release_digest)))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let view = body_json(response).await;
	assert_eq!(view["advisories"].as_array().expect("advisories").len(), 1);
	assert_eq!(view["advisories"][0]["provider_id"], "scanner");
	assert_eq!(view["advisories"][0]["block_promotion"], true);

	let unknown = Advisory {
		provider_id: "ghost".to_string(),
		..advisory
	};
	let signed = sign_payload(Kind::Advisory, &unknown, &[&provider]);
	let request = axum::http::Request::post("/v1/advisories")
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn federation_syncs_a_game_definition() {
	let (home, _home_directory) = app().await;
	let key = key(16);
	let request = axum::http::Request::post("/v1/games")
		.body(Body::from(game_genesis_wire(&key)))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let game_id = body_json(response).await["id"].as_str().expect("game id").to_string();

	let request = axum::http::Request::post(format!("/v1/games/{game_id}/definitions"))
		.body(Body::from(game_definition_wire(&key, &game_id)))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	let serving = home.clone();
	tokio::spawn(async move {
		let _ = axum::serve(listener, serving).await;
	});

	let (directory, _directory_dir) = app_mode(crate::config::Publishing::Open, true).await;
	let (session, csrf) = login(&directory, "ops@example.org").await;
	let sync = axum::http::Request::post("/v1/federation/sync-definition")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(
			serde_json::json!({
				"home_url": format!("http://127.0.0.1:{}", address.port()),
				"id": game_id,
				"kind": "game",
			})
			.to_string(),
		))
		.expect("request");
	let response = directory.clone().oneshot(sync).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let get = axum::http::Request::get(format!("/v1/games/{game_id}"))
		.body(Body::empty())
		.expect("request");
	let response = directory.oneshot(get).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["payload"]["display_name"], "Minecraft");
	assert_eq!(view["payload"]["version_ordering"], "semver");
}
