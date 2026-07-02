use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_crypto::ObjectKind;
use moraine_model::Canonical;
use moraine_model::delegation::{Delegation, KeyDelegation};
use moraine_model::feed::FeedEntry;
use moraine_model::signed::SignedObject;
use moraine_model::trust::{RootSet, verify_key_delegation};
use serde::{Deserialize, Serialize};

use crate::routes::AppState;
use crate::store::{FeedRow, StoredObject};
use crate::verify::{self, VerifyError};

const OBJECT_CONTENT_TYPE: &str = "application/vnd.moraine.object+cbor";

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/projects", post(create_project))
		.route("/v1/projects/{id}", get(project_summary))
		.route("/v1/projects/{id}/objects/{kind}", post(store_object))
		.route("/v1/projects/{id}/feed", get(feed_page).post(append_feed))
		.route("/v1/objects/{hex}", get(object_bytes))
}

#[derive(Serialize)]
struct ProjectReceipt {
	project_id: String,
	genesis: String,
}

#[derive(Serialize)]
struct ProjectSummary {
	project_id: String,
	genesis: String,
	head_seq: i64,
	head_entry: Option<String>,
	profile: Option<String>,
}

#[derive(Serialize)]
struct ObjectReceipt {
	id: String,
	kind: String,
}

#[derive(Serialize)]
struct FeedReceipt {
	seq: i64,
	entry: String,
}

#[derive(Serialize)]
struct FeedEntryView {
	seq: i64,
	kind: String,
	object: String,
	entry: String,
	declared_at: i64,
	previous: Option<String>,
}

#[derive(Serialize)]
struct FeedPage {
	project_id: String,
	head_seq: i64,
	entries: Vec<FeedEntryView>,
	next: Option<i64>,
}

#[derive(Deserialize)]
struct FeedQuery {
	#[serde(default)]
	after: i64,
	limit: Option<i64>,
}

async fn create_project(State(state): State<AppState>, body: Bytes) -> Response {
	let (_, object) = match verify::verify_genesis(&body) {
		Ok(verified) => verified,
		Err(error) => return bad_request(error),
	};
	let digest = object.digest.to_vec();
	match state.metadata.project(&object.id).await {
		Ok(Some(existing)) if existing.genesis_digest != digest => {
			return (StatusCode::CONFLICT, "project id already bound to a different genesis").into_response();
		}
		Ok(Some(_)) => {}
		Ok(None) => {
			if let Err(error) = state.metadata.put_object(&stored(&object)).await {
				return storage_error(error);
			}
			if let Err(error) = state.metadata.create_project(&object.id, &digest).await {
				return storage_error(error);
			}
		}
		Err(error) => return storage_error(error),
	}
	(
		StatusCode::CREATED,
		Json(ProjectReceipt {
			project_id: object.id,
			genesis: id_for(&digest),
		}),
	)
		.into_response()
}

async fn project_summary(State(state): State<AppState>, Path(id): Path<String>) -> Response {
	match state.metadata.project(&id).await {
		Ok(Some(project)) => {
			let summary = ProjectSummary {
				project_id: project.id,
				genesis: id_for(&project.genesis_digest),
				head_seq: project.head_seq,
				head_entry: project.head_digest.as_deref().map(id_for),
				profile: project.profile_digest.as_deref().map(id_for),
			};
			Json(summary).into_response()
		}
		Ok(None) => (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => storage_error(error),
	}
}

async fn store_object(State(state): State<AppState>, Path((id, kind)): Path<(String, String)>, body: Bytes) -> Response {
	let Some(kind) = ObjectKind::parse(&kind) else {
		return (StatusCode::BAD_REQUEST, "unknown object kind").into_response();
	};
	if matches!(kind, ObjectKind::Genesis | ObjectKind::FeedEntry) {
		return (StatusCode::BAD_REQUEST, "genesis and feed entries use their own endpoints").into_response();
	}
	let root = match load_root(&state, &id).await {
		Ok(root) => root,
		Err(response) => return *response,
	};
	let delegations = match load_delegations(&state, &id, &root).await {
		Ok(delegations) => delegations,
		Err(response) => return *response,
	};
	let object = match verify::verify_object_authorized(kind, &body, &root, &delegations, unix_now()) {
		Ok(object) => object,
		Err(error) => return bad_request(error),
	};
	if let Err(error) = state.metadata.put_object(&stored(&object)).await {
		return storage_error(error);
	}
	let receipt = ObjectReceipt {
		id: object.id,
		kind: kind.as_str().to_string(),
	};
	(StatusCode::CREATED, Json(receipt)).into_response()
}

async fn append_feed(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
	let project = match state.metadata.project(&id).await {
		Ok(Some(project)) => project,
		Ok(None) => return (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => return storage_error(error),
	};
	let root = match load_root(&state, &id).await {
		Ok(root) => root,
		Err(response) => return *response,
	};
	let delegations = match load_delegations(&state, &id, &root).await {
		Ok(delegations) => delegations,
		Err(response) => return *response,
	};
	let object = match verify::verify_object_authorized(ObjectKind::FeedEntry, &body, &root, &delegations, unix_now()) {
		Ok(object) => object,
		Err(error) => return bad_request(error),
	};
	let entry = match FeedEntry::from_canonical_bytes(&object.payload_bytes) {
		Ok(entry) => entry,
		Err(error) => return bad_request(VerifyError::Decode(error)),
	};

	let expected_seq = project.head_seq + 1;
	if entry.sequence as i64 != expected_seq {
		return (StatusCode::CONFLICT, "feed entry is not the next sequence").into_response();
	}
	let previous_matches = match (&project.head_digest, &entry.previous) {
		(None, None) => expected_seq == 1,
		(Some(head), Some(previous)) => previous == head,
		_ => false,
	};
	if !previous_matches {
		return (StatusCode::CONFLICT, "feed entry does not link to the current head").into_response();
	}
	match state.metadata.object(&entry.object_digest).await {
		Ok(Some(_)) => {}
		Ok(None) => return (StatusCode::CONFLICT, "referenced object is not stored").into_response(),
		Err(error) => return storage_error(error),
	}

	if let Err(error) = state.metadata.put_object(&stored(&object)).await {
		return storage_error(error);
	}
	let row = FeedRow {
		project_id: project.id,
		seq: entry.sequence as i64,
		previous: entry.previous,
		entry_digest: object.digest.to_vec(),
		kind: entry.kind,
		object_digest: entry.object_digest,
		payload: object.payload_bytes,
		wire: object.wire_bytes,
	};
	if let Err(error) = state.metadata.append_feed(&row).await {
		return storage_error(error);
	}
	(
		StatusCode::CREATED,
		Json(FeedReceipt {
			seq: row.seq,
			entry: id_for(&object.digest),
		}),
	)
		.into_response()
}

async fn feed_page(State(state): State<AppState>, Path(id): Path<String>, Query(query): Query<FeedQuery>) -> Response {
	let project = match state.metadata.project(&id).await {
		Ok(Some(project)) => project,
		Ok(None) => return (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => return storage_error(error),
	};
	let limit = query
		.limit
		.unwrap_or(state.capability.max_feed_page_entries as i64)
		.clamp(1, state.capability.max_feed_page_entries as i64);
	let rows = match state.metadata.feed_after(&id, query.after, limit).await {
		Ok(rows) => rows,
		Err(error) => return storage_error(error),
	};
	let mut entries = Vec::with_capacity(rows.len());
	for row in &rows {
		let declared_at = FeedEntry::from_canonical_bytes(&row.payload)
			.map(|entry| entry.declared_at)
			.unwrap_or(0);
		entries.push(FeedEntryView {
			seq: row.seq,
			kind: row.kind.clone(),
			object: id_for(&row.object_digest),
			entry: id_for(&row.entry_digest),
			declared_at,
			previous: row.previous.as_deref().map(id_for),
		});
	}
	let next = entries.last().map(|entry| entry.seq);
	let page = FeedPage {
		project_id: project.id,
		head_seq: project.head_seq,
		entries,
		next,
	};
	Json(page).into_response()
}

async fn object_bytes(State(state): State<AppState>, Path(hex_digest): Path<String>) -> Response {
	let Some(digest) = parse_hex_digest(&hex_digest) else {
		return (StatusCode::BAD_REQUEST, "invalid digest").into_response();
	};
	match state.metadata.object(&digest).await {
		Ok(Some(object)) => {
			let mut response = Response::new(Body::from(object.wire));
			let headers = response.headers_mut();
			headers.insert(header::CONTENT_TYPE, OBJECT_CONTENT_TYPE.parse().expect("valid header"));
			headers.insert(
				header::CACHE_CONTROL,
				"public, max-age=31536000, immutable".parse().expect("valid header"),
			);
			headers.insert(
				header::CONTENT_LENGTH,
				object.payload.len().to_string().parse().expect("valid header"),
			);
			response
		}
		Ok(None) => (StatusCode::NOT_FOUND, "no such object").into_response(),
		Err(error) => storage_error(error),
	}
}

async fn load_root(state: &AppState, project_id: &str) -> Result<RootSet, Box<Response>> {
	let project = match state.metadata.project(project_id).await {
		Ok(Some(project)) => project,
		Ok(None) => return Err(Box::new((StatusCode::NOT_FOUND, "no such project").into_response())),
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	let genesis = match state.metadata.object(&project.genesis_digest).await {
		Ok(Some(genesis)) => genesis,
		Ok(None) => {
			return Err(Box::new(
				(StatusCode::INTERNAL_SERVER_ERROR, "project genesis is missing").into_response(),
			));
		}
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	verify::verify_genesis(&genesis.wire)
		.map(|(root, _)| root)
		.map_err(|error| Box::new(bad_request(error)))
}

async fn load_delegations(state: &AppState, project_id: &str, root: &RootSet) -> Result<Vec<KeyDelegation>, Box<Response>> {
	let stored = match state.metadata.objects_of_kind("delegation", 500).await {
		Ok(stored) => stored,
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	let mut delegations = Vec::new();
	for object in stored {
		let Ok(signed) = SignedObject::<Delegation>::from_bytes(&object.wire) else {
			continue;
		};
		let Delegation::Key(key) = &signed.payload else {
			continue;
		};
		if key.project_id != project_id {
			continue;
		}
		if verify_key_delegation(&signed, root).is_err() {
			continue;
		}
		delegations.push(key.clone());
	}
	Ok(delegations)
}

fn unix_now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

fn stored(object: &verify::VerifiedObject) -> StoredObject {
	StoredObject {
		digest: object.digest.to_vec(),
		kind: object.kind.as_str().to_string(),
		payload: object.payload_bytes.clone(),
		wire: object.wire_bytes.clone(),
	}
}

fn id_for(digest: &[u8]) -> String {
	format!("gd:sha256:{}", hex::encode(digest))
}

fn parse_hex_digest(value: &str) -> Option<[u8; 32]> {
	hex::decode(value).ok()?.try_into().ok()
}

fn bad_request(error: VerifyError) -> Response {
	(StatusCode::BAD_REQUEST, error.to_string()).into_response()
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "metadata store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}

#[cfg(test)]
mod tests {
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
}
