pub mod advisories;
pub mod artifacts;
pub mod compatibility;
pub mod definitions;
pub mod review;
pub mod search;
pub mod views;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_crypto::{ObjectKind, object_id};
use moraine_model::Canonical;
use moraine_model::delegation::{Delegation, KeyDelegation};
use moraine_model::feed::FeedEntry;
use moraine_model::signed::SignedObject;
use moraine_model::trust::{RootSet, verify_key_delegation, verify_ownership_transfer};
use serde::{Deserialize, Serialize};

use crate::db::{FeedRow, ProjectRow, StoredObject};
use crate::registry::views::describe_stored;
use crate::routes::AppState;
use crate::verify::{self, VerifyError};

pub(crate) const OBJECT_CONTENT_TYPE: &str = "application/vnd.moraine.object+cbor";

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/projects", post(create_project))
		.route("/v1/projects/{id}", get(project_summary))
		.route("/v1/projects/{id}/objects/{kind}", post(store_object))
		.route("/v1/projects/{id}/feed", get(feed_page).post(append_feed))
		.route("/v1/projects/{id}/transfer", post(transfer))
		.merge(crate::registry::views::routes())
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
	owner: Option<OwnerView>,
}

#[derive(Serialize)]
struct OwnerView {
	kind: String,
	id: String,
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
	title: Option<String>,
	release: Option<crate::registry::views::ReleaseSummary>,
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
	truncated: bool,
}

#[derive(Deserialize)]
struct FeedQuery {
	#[serde(default)]
	after: i64,
	limit: Option<i64>,
	#[serde(default)]
	game_version: Option<String>,
	#[serde(default)]
	loader: Option<String>,
	#[serde(default)]
	loader_version: Option<String>,
}

async fn create_project(State(state): State<AppState>, body: Bytes) -> Response {
	let (_, object) = match verify::verify_genesis(&body) {
		Ok(verified) => verified,
		Err(error) => return bad_request(&state, error),
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
			let owner = match (project.owner_kind, project.owner_id) {
				(Some(kind), Some(id)) => Some(OwnerView { kind, id }),
				_ => None,
			};
			let summary = ProjectSummary {
				project_id: project.id,
				genesis: id_for(&project.genesis_digest),
				head_seq: project.head_seq,
				head_entry: project.head_digest.as_deref().map(id_for),
				profile: project.profile_digest.as_deref().map(id_for),
				owner,
			};
			Json(summary).into_response()
		}
		Ok(None) => (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Serialize)]
struct TransferReceipt {
	transfer: String,
}

async fn transfer(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
	match state.metadata.project(&id).await {
		Ok(Some(_)) => {}
		Ok(None) => return (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => return storage_error(error),
	}
	let root = match load_root(&state, &id).await {
		Ok(root) => root,
		Err(response) => return *response,
	};
	let signed = match SignedObject::<Delegation>::from_bytes(&body) {
		Ok(signed) => signed,
		Err(error) => return bad_request(&state, VerifyError::Decode(error)),
	};
	if !matches!(signed.payload, Delegation::OwnershipTransfer(_)) {
		return (StatusCode::BAD_REQUEST, "object is not an ownership transfer").into_response();
	}
	if let Err(error) = verify_ownership_transfer(&signed, &root) {
		return bad_request(&state, VerifyError::Signature(error));
	}
	let digest = object_id(ObjectKind::Delegation, &signed.payload_bytes);
	let stored = StoredObject {
		digest: digest.to_vec(),
		kind: "delegation".to_string(),
		payload: signed.payload_bytes,
		wire: body.to_vec(),
	};
	if let Err(error) = state.metadata.put_object(&stored).await {
		return storage_error(error);
	}
	(
		StatusCode::CREATED,
		Json(TransferReceipt {
			transfer: id_for(&digest),
		}),
	)
		.into_response()
}

async fn apply_withdrawal(state: &AppState, project_id: &str, object_digest: &[u8]) -> Result<(), Box<Response>> {
	let object = match state.metadata.object(object_digest).await {
		Ok(Some(object)) => object,
		Ok(None) => {
			return Err(Box::new(
				(StatusCode::CONFLICT, "withdrawal object is not stored").into_response(),
			));
		}
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	let signed = match SignedObject::<moraine_model::release::ReleaseObject>::from_bytes(&object.wire) {
		Ok(signed) => signed,
		Err(error) => return Err(Box::new(bad_request(state, VerifyError::Decode(error)))),
	};
	let moraine_model::release::ReleaseObject::Withdrawal(withdrawal) = signed.payload else {
		return Err(Box::new(
			(StatusCode::BAD_REQUEST, "object is not a withdrawal").into_response(),
		));
	};
	match state
		.metadata
		.record_withdrawal(
			project_id,
			&withdrawal.release_id,
			&withdrawal.reason,
			withdrawal.note.as_deref(),
			withdrawal.declared_time,
		)
		.await
	{
		Ok(()) => Ok(()),
		Err(error) => Err(Box::new(storage_error(error))),
	}
}

async fn apply_ownership_transfer(state: &AppState, project_id: &str, object_digest: &[u8]) -> Result<(), Box<Response>> {
	let project = match state.metadata.project(project_id).await {
		Ok(Some(project)) => project,
		Ok(None) => return Err(Box::new((StatusCode::NOT_FOUND, "no such project").into_response())),
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	let object = match state.metadata.object(object_digest).await {
		Ok(Some(object)) => object,
		Ok(None) => {
			return Err(Box::new(
				(StatusCode::CONFLICT, "transfer object is not stored").into_response(),
			));
		}
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	let signed = match SignedObject::<Delegation>::from_bytes(&object.wire) {
		Ok(signed) => signed,
		Err(error) => return Err(Box::new(bad_request(state, VerifyError::Decode(error)))),
	};
	let Delegation::OwnershipTransfer(record) = &signed.payload else {
		return Err(Box::new(
			(StatusCode::BAD_REQUEST, "object is not an ownership transfer").into_response(),
		));
	};
	let root = load_root(state, project_id).await?;
	if let Err(error) = verify_ownership_transfer(&signed, &root) {
		return Err(Box::new(bad_request(state, VerifyError::Signature(error))));
	}
	if let (Some(current_kind), Some(current_id)) = (&project.owner_kind, &project.owner_id)
		&& (current_kind != &record.from_owner.kind || current_id != &record.from_owner.id)
	{
		return Err(Box::new(
			(StatusCode::CONFLICT, "the transfer does not start from the current owner").into_response(),
		));
	}
	match state
		.metadata
		.set_project_owner(project_id, &record.to_owner.kind, &record.to_owner.id)
		.await
	{
		Ok(_) => Ok(()),
		Err(error) => Err(Box::new(storage_error(error))),
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
		Err(error) => return bad_request(&state, error),
	};
	if let Err(error) = store_object_record(&state, &object).await {
		return storage_error(error);
	}
	let receipt = ObjectReceipt {
		id: object.id,
		kind: kind.as_str().to_string(),
	};
	(StatusCode::CREATED, Json(receipt)).into_response()
}

async fn append_feed(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
	if !state.capability.is_open() {
		return (
			StatusCode::CONFLICT,
			"publishing is under review; submit through /v1/submissions",
		)
			.into_response();
	}
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
		Err(error) => return Err(Box::new(bad_request(state, error))),
	};
	let entry = match FeedEntry::from_canonical_bytes(&object.payload_bytes) {
		Ok(entry) => entry,
		Err(error) => return Err(Box::new(bad_request(state, VerifyError::Decode(error)))),
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
	if row.kind == "profile-updated"
		&& let Err(error) = crate::registry::search::refresh_search_document(state, &row.object_digest).await
	{
		return Err(Box::new(storage_error(error)));
	}
	if row.kind == "ownership-transferred" {
		apply_ownership_transfer(state, &row.project_id, &row.object_digest).await?;
	}
	if row.kind == "release-withdrawn" {
		apply_withdrawal(state, &row.project_id, &row.object_digest).await?;
	}
	if let Err(error) =
		crate::federation::notifications::notify_followers(state, &row.project_id, &row.kind, &row.object_digest, row.seq)
			.await
	{
		return Err(Box::new(storage_error(error)));
	}
	if let Err(error) =
		crate::federation::webhooks::enqueue_event(state, &row.kind, &row.project_id, &row.object_digest, row.seq).await
	{
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
	let mut entries = Vec::with_capacity(limit as usize);
	let mut scheme: Option<Option<moraine_model::version::OrderingScheme>> = None;
	let mut loader_scheme: Option<Option<moraine_model::version::OrderingScheme>> = None;
	let mut scanned = query.after;
	let mut pages = 0;
	loop {
		let rows = match state.metadata.feed_after(&id, scanned, limit).await {
			Ok(rows) => rows,
			Err(error) => return storage_error(error),
		};
		if rows.is_empty() {
			break;
		}
		let fetched = rows.len() as i64;
		scanned = rows.last().map(|row| row.seq).unwrap_or(scanned);
		for row in &rows {
			if entries.len() as i64 >= limit {
				break;
			}
			let declared_at = FeedEntry::from_canonical_bytes(&row.payload)
				.map(|entry| entry.declared_at)
				.unwrap_or(0);
			let object = match state.metadata.object(&row.object_digest).await {
				Ok(Some(object)) => Some(object),
				_ => None,
			};
			if let Some(object) = object.as_ref()
				&& object.kind == "release"
			{
				if let Some(version) = query.game_version.as_deref() {
					let ordering = match scheme {
						Some(ordering) => ordering,
						None => {
							let resolved = crate::registry::compatibility::game_ordering(&state, object).await;
							scheme = Some(resolved);
							resolved
						}
					};
					if !crate::registry::compatibility::release_matches_game_version(object, version, ordering) {
						continue;
					}
				}
				if let Some(loader) = query.loader.as_deref()
					&& !crate::registry::compatibility::release_declares_loader(object, loader)
				{
					continue;
				}
				if let (Some(loader), Some(version)) = (query.loader.as_deref(), query.loader_version.as_deref()) {
					let ordering = match loader_scheme {
						Some(ordering) => ordering,
						None => {
							let resolved = crate::registry::compatibility::loader_ordering(&state, loader).await;
							loader_scheme = Some(resolved);
							resolved
						}
					};
					if !crate::registry::compatibility::release_matches_loader_version(object, loader, version, ordering) {
						continue;
					}
				}
			}
			let (title, release) = match object.as_ref() {
				Some(object) => (describe_stored(object), crate::registry::views::summarize_release(object)),
				_ => (None, None),
			};
			entries.push(FeedEntryView {
				seq: row.seq,
				kind: row.kind.clone(),
				title,
				release,
				object: id_for(&row.object_digest),
				entry: id_for(&row.entry_digest),
				declared_at,
				previous: row.previous.as_deref().map(id_for),
			});
		}
		pages += 1;
		if (entries.len() as i64) >= limit
			|| fetched < limit
			|| pages >= state.capability.max_feed_scan_pages.max(1) as usize
		{
			break;
		}
	}
	let next = match entries.last() {
		Some(entry) => Some(entry.seq),
		None if scanned > query.after => Some(scanned),
		None => None,
	};
	let truncated = (entries.len() as i64) < limit
		&& scanned > query.after
		&& pages >= state.capability.max_feed_scan_pages.max(1) as usize;
	let page = FeedPage {
		project_id: project.id,
		head_seq: project.head_seq,
		entries,
		next,
		truncated,
	};
	Json(page).into_response()
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
		.map_err(|error| Box::new(bad_request(state, error)))
}

pub(crate) async fn load_delegations(
	state: &AppState,
	project_id: &str,
	root: &RootSet,
) -> Result<Vec<KeyDelegation>, Box<Response>> {
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

pub(crate) fn stored(object: &verify::VerifiedObject) -> StoredObject {
	StoredObject {
		digest: object.digest.to_vec(),
		kind: object.kind.as_str().to_string(),
		payload: object.payload_bytes.clone(),
		wire: object.wire_bytes.clone(),
	}
}

pub(crate) fn id_for(digest: &[u8]) -> String {
	format!("gd:sha256:{}", hex::encode(digest))
}

pub(crate) fn parse_hex_digest(value: &str) -> Option<[u8; 32]> {
	hex::decode(value).ok()?.try_into().ok()
}

pub(crate) async fn store_object_record(state: &AppState, object: &verify::VerifiedObject) -> Result<(), sqlx::Error> {
	state.metadata.put_object(&stored(object)).await?;
	if object.kind == ObjectKind::Release
		&& let Ok(release_object) = moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload_bytes)
	{
		match release_object {
			moraine_model::release::ReleaseObject::Release(release) => {
				for artifact in &release.artifacts {
					state
						.metadata
						.index_artifact(&artifact.digest, &release.project_id, &object.digest)
						.await?;
				}
				let loaders: Vec<String> = release
					.compatibility
					.iter()
					.filter_map(|entry| entry.loader_id.clone())
					.collect();
				if !loaders.is_empty() {
					state
						.metadata
						.add_search_labels(&release.project_id, "loader", &loaders)
						.await?;
				}
			}
			moraine_model::release::ReleaseObject::Location(location) => {
				for entry in &location.locations {
					state
						.metadata
						.index_location(
							&location.artifact_digest,
							&entry.url,
							entry.kind.as_str(),
							entry.operator_id.as_deref(),
							&object.digest,
						)
						.await?;
				}
			}
			moraine_model::release::ReleaseObject::Withdrawal(_) => {}
		}
	}
	Ok(())
}

fn bad_request(state: &AppState, error: VerifyError) -> Response {
	if matches!(error, VerifyError::Signature(_)) {
		state.metrics.record_signature_failure();
	}
	(StatusCode::BAD_REQUEST, error.to_string()).into_response()
}

pub(crate) fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "metadata store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
