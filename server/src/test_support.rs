use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::header;
use axum::response::Response;
use moraine_crypto::{ObjectKind as Kind, SigningKey, object_id};
use moraine_model::artifact::Artifact;
use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
use moraine_model::definition::{GameDef, VersionSyntax};
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::release::ReleasePayload;
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use crate::blob::BlobStore;
use crate::capability::Capability;
use crate::routes::AppState;
use crate::store::MetadataStore;

pub(crate) fn key(byte: u8) -> SigningKey {
	SigningKey::from_seed(&[byte; 32])
}

pub(crate) fn sample_id(label: &str) -> String {
	format!(
		"gd:sha256:{}",
		hex::encode(moraine_crypto::object_id(Kind::Release, label.as_bytes()))
	)
}

pub(crate) const PROJECT_KINDS: &[&str] = &["delegation", "release", "profile"];

pub(crate) fn game_genesis_wire(key: &SigningKey) -> Vec<u8> {
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

pub(crate) fn game_definition_wire(key: &SigningKey, game_id: &str) -> Vec<u8> {
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

pub(crate) fn genesis_wire(signer: &SigningKey, kinds: &[&str]) -> Vec<u8> {
	genesis_wire_roots(&[signer], kinds)
}

pub(crate) fn genesis_wire_roots(roots: &[&SigningKey], kinds: &[&str]) -> Vec<u8> {
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

pub(crate) fn release_wire(signer: &SigningKey, project_id: &str) -> (Vec<u8>, [u8; 32]) {
	release_wire_variant(signer, project_id, 0x42, "1.0.0")
}

pub(crate) fn release_wire_variant(
	signer: &SigningKey,
	project_id: &str,
	nonce: u8,
	human_version: &str,
) -> (Vec<u8>, [u8; 32]) {
	release_wire_for_game(signer, project_id, nonce, human_version, "1.20.1")
}

pub(crate) fn release_wire_for_game(
	signer: &SigningKey,
	project_id: &str,
	nonce: u8,
	human_version: &str,
	game_version: &str,
) -> (Vec<u8>, [u8; 32]) {
	release_wire_for_game_with_loader(signer, project_id, nonce, human_version, game_version, Some("fabric"), None)
}

pub(crate) fn release_wire_for_game_with_loader(
	signer: &SigningKey,
	project_id: &str,
	nonce: u8,
	human_version: &str,
	game_version: &str,
	loader: Option<&str>,
	loader_version: Option<&str>,
) -> (Vec<u8>, [u8; 32]) {
	let release = ReleasePayload {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: sample_id("minecraft"),
		release_nonce: vec![nonce; 16],
		human_version: human_version.to_string(),
		channel: "release".to_string(),
		kind: "mod".to_string(),
		declared_time: 1_760_000_000,
		compatibility: vec![Compatibility {
			game_version_predicate: Predicate::new(Scheme::Exact, vec![game_version.to_string()]),
			loader_id: loader.map(sample_id),
			loader_version_predicate: loader_version.map(|version| Predicate::new(Scheme::Exact, vec![version.to_string()])),
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

pub(crate) fn release_wire_no_loader(
	signer: &SigningKey,
	project_id: &str,
	nonce: u8,
	human_version: &str,
) -> (Vec<u8>, [u8; 32]) {
	release_wire_for_game_with_loader(signer, project_id, nonce, human_version, "1.20.1", None, None)
}

pub(crate) fn feed_wire(
	signer: &SigningKey,
	project_id: &str,
	sequence: u64,
	previous: Option<[u8; 32]>,
	object_digest: [u8; 32],
) -> Vec<u8> {
	feed_wire_kind(signer, project_id, sequence, previous, object_digest, "release-published")
}

pub(crate) fn feed_wire_kind(
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
	let application = app_in_with_scan(
		directory.path(),
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
	let application = app_in(
		directory.path(),
		publishing,
		allow_insecure_federation_local,
		max_feed_page_entries,
		requests_per_minute,
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
	publishing: crate::config::Publishing,
	allow_insecure_federation_local: bool,
	max_feed_page_entries: u32,
	requests_per_minute: u32,
	max_feed_scan_pages: u32,
) -> Router {
	let store = Arc::new(BlobStore::new(directory).await.expect("blob store"));
	let metadata = Arc::new(
		MetadataStore::open(directory.join("metadata.sqlite"))
			.await
			.expect("metadata"),
	);
	let config = crate::config::Config {
		bind: "127.0.0.1:0".parse().expect("addr"),
		data_dir: directory.to_path_buf(),
		max_artifact_bytes: 1024,
		max_feed_page_entries,
		max_feed_scan_pages,
		skip_migrate_on_start: false,
		database_url: None,
		max_response_bytes: 16_777_216,
		staging_retention_seconds: 3_600,
		blob_retention_seconds: 604_800,
		max_sync_pages: 200,
		requests_per_minute,
		max_concurrent_syncs: 4,
		tls_extra_roots: None,
		allow_insecure_federation_local,
		publishing,
		web_dir: None,
	};
	let state = AppState {
		store,
		metadata,
		capability: Arc::new(Capability::discover(&config)),
		login_limiter: std::sync::Arc::new(crate::auth::LoginLimiter::new()),
		metrics: std::sync::Arc::new(crate::metrics::Metrics::new()),
		rate_limiter: std::sync::Arc::new(crate::ratelimit::RateLimiter::new()),
		web_dir: None,
	};
	crate::routes::router(state)
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
