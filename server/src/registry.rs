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
use crate::store::{FeedRow, ProjectRow, StoredObject};
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
	match ingest_feed(&state, &id, &body).await {
		Ok((seq, entry)) => (StatusCode::CREATED, Json(FeedReceipt { seq, entry })).into_response(),
		Err(response) => *response,
	}
}

pub(crate) struct PreparedFeed {
	pub project: ProjectRow,
	pub entry: FeedEntry,
	pub object: verify::VerifiedObject,
}

/// Verifies a feed entry, its authorization, and that the referenced object is
/// stored. Continuity against the head is checked separately, at the moment of
/// commit, because the head can move between submission and acceptance.
pub(crate) async fn prepare_feed(state: &AppState, project_id: &str, body: &[u8]) -> Result<PreparedFeed, Box<Response>> {
	let project = match state.metadata.project(project_id).await {
		Ok(Some(project)) => project,
		Ok(None) => return Err(Box::new((StatusCode::NOT_FOUND, "no such project").into_response())),
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	let root = load_root(state, project_id).await?;
	let delegations = load_delegations(state, project_id, &root).await?;
	let object = match verify::verify_object_authorized(ObjectKind::FeedEntry, body, &root, &delegations, unix_now()) {
		Ok(object) => object,
		Err(error) => return Err(Box::new(bad_request(error))),
	};
	let entry = match FeedEntry::from_canonical_bytes(&object.payload_bytes) {
		Ok(entry) => entry,
		Err(error) => return Err(Box::new(bad_request(VerifyError::Decode(error)))),
	};
	match state.metadata.object(&entry.object_digest).await {
		Ok(Some(_)) => {}
		Ok(None) => {
			return Err(Box::new(
				(StatusCode::CONFLICT, "referenced object is not stored").into_response(),
			));
		}
		Err(error) => return Err(Box::new(storage_error(error))),
	}
	Ok(PreparedFeed { project, entry, object })
}

pub(crate) async fn ingest_feed(state: &AppState, project_id: &str, body: &[u8]) -> Result<(i64, String), Box<Response>> {
	let prepared = prepare_feed(state, project_id, body).await?;
	let entry = prepared.entry;
	let object = prepared.object;

	let expected_seq = prepared.project.head_seq + 1;
	if entry.sequence as i64 != expected_seq {
		return Err(Box::new(
			(StatusCode::CONFLICT, "feed entry is not the next sequence").into_response(),
		));
	}
	let previous_matches = match (&prepared.project.head_digest, &entry.previous) {
		(None, None) => expected_seq == 1,
		(Some(head), Some(previous)) => previous == head,
		_ => false,
	};
	if !previous_matches {
		return Err(Box::new(
			(StatusCode::CONFLICT, "feed entry does not link to the current head").into_response(),
		));
	}
	let entry_id = id_for(&object.digest);
	if let Err(error) = state.metadata.put_object(&stored(&object)).await {
		return Err(Box::new(storage_error(error)));
	}
	let row = FeedRow {
		project_id: prepared.project.id,
		seq: entry.sequence as i64,
		previous: entry.previous,
		entry_digest: object.digest.to_vec(),
		kind: entry.kind,
		object_digest: entry.object_digest,
		payload: object.payload_bytes,
		wire: object.wire_bytes,
	};
	if let Err(error) = state.metadata.append_feed(&row).await {
		return Err(Box::new(storage_error(error)));
	}
	Ok((row.seq, entry_id))
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
#[path = "registry_tests.rs"]
mod tests;
