use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_crypto::ObjectKind;
use moraine_model::Canonical;
use moraine_model::advisory::Advisory;
use moraine_model::delegation::{Delegation, KeyDelegation};
use moraine_model::feed::FeedEntry;
use moraine_model::profile::ProfileRevision;
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
		.route("/v1/projects/{id}/profile", get(project_profile))
		.route("/v1/projects/{id}/releases/{hex}", get(release_view))
		.route("/v1/lookup", get(lookup))
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
	title: Option<String>,
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
	if row.kind == "profile-updated"
		&& let Err(error) = crate::search::refresh_search_document(state, &row.object_digest).await
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
	let rows = match state.metadata.feed_after(&id, query.after, limit).await {
		Ok(rows) => rows,
		Err(error) => return storage_error(error),
	};
	let mut entries = Vec::with_capacity(rows.len());
	for row in &rows {
		let declared_at = FeedEntry::from_canonical_bytes(&row.payload)
			.map(|entry| entry.declared_at)
			.unwrap_or(0);
		let title = match state.metadata.object(&row.object_digest).await {
			Ok(Some(object)) => describe_stored(&object),
			_ => None,
		};
		entries.push(FeedEntryView {
			seq: row.seq,
			kind: row.kind.clone(),
			title,
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

#[derive(Deserialize)]
struct LookupQuery {
	sha256: String,
}

#[derive(Serialize)]
struct LookupView {
	digest: String,
	matches: Vec<LookupMatch>,
}

#[derive(Serialize)]
struct LookupMatch {
	project_id: String,
	release: String,
	human_version: Option<String>,
	filename: Option<String>,
}

/// Stores a verified object and, for a release, indexes each artifact digest so
/// a file can be resolved back to the release that published it.
pub(crate) async fn store_object_record(state: &AppState, object: &verify::VerifiedObject) -> Result<(), sqlx::Error> {
	state.metadata.put_object(&stored(object)).await?;
	if object.kind == ObjectKind::Release
		&& let Ok(moraine_model::release::ReleaseObject::Release(release)) =
			moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload_bytes)
	{
		for artifact in &release.artifacts {
			state
				.metadata
				.index_artifact(&artifact.digest, &release.project_id, &object.digest)
				.await?;
		}
	}
	Ok(())
}

async fn lookup(State(state): State<AppState>, Query(query): Query<LookupQuery>) -> Response {
	let Some(digest) = parse_sha256(&query.sha256) else {
		return (StatusCode::BAD_REQUEST, "expected a sha256 digest").into_response();
	};
	let matches = match state.metadata.artifacts_for_digest(&digest).await {
		Ok(matches) => matches,
		Err(error) => return storage_error(error),
	};
	let mut views = Vec::with_capacity(matches.len());
	for entry in matches {
		let mut human_version = None;
		let mut filename = None;
		if let Ok(Some(object)) = state.metadata.object(&entry.release_digest).await
			&& let Ok(moraine_model::release::ReleaseObject::Release(release)) =
				moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload)
		{
			human_version = Some(release.human_version);
			filename = release
				.artifacts
				.iter()
				.find(|artifact| artifact.digest == digest)
				.map(|artifact| artifact.filename.clone());
		}
		views.push(LookupMatch {
			project_id: entry.project_id,
			release: id_for(&entry.release_digest),
			human_version,
			filename,
		});
	}
	Json(LookupView {
		digest: format!("sha256:{}", hex::encode(digest)),
		matches: views,
	})
	.into_response()
}

fn parse_sha256(value: &str) -> Option<[u8; 32]> {
	let hex = value.strip_prefix("sha256:").unwrap_or(value);
	hex::decode(hex).ok()?.try_into().ok()
}

#[derive(Serialize)]
struct ProfileView {
	project_id: String,
	display_name: String,
	summary: String,
	description: String,
	categories: Vec<String>,
	tags: Vec<String>,
	links: Vec<LinkView>,
	communities: Vec<LinkView>,
	revision: String,
}

#[derive(Serialize)]
struct LinkView {
	kind: String,
	url: String,
}

async fn project_profile(State(state): State<AppState>, Path(id): Path<String>) -> Response {
	let project = match state.metadata.project(&id).await {
		Ok(Some(project)) => project,
		Ok(None) => return (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => return storage_error(error),
	};
	let Some(revision_digest) = project.profile_digest else {
		return (StatusCode::NOT_FOUND, "no profile published").into_response();
	};
	let Some(object) = (match state.metadata.object(&revision_digest).await {
		Ok(object) => object,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "profile object is missing").into_response();
	};
	let Ok(profile) = ProfileRevision::from_canonical_bytes(&object.payload) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "stored profile does not decode").into_response();
	};
	let view = ProfileView {
		project_id: profile.project_id,
		display_name: profile.display_name,
		summary: profile.summary,
		description: profile.description,
		categories: profile.categories,
		tags: profile.tags,
		links: profile
			.links
			.into_iter()
			.map(|link| LinkView {
				kind: link.kind,
				url: link.url,
			})
			.collect(),
		communities: profile
			.communities
			.into_iter()
			.map(|link| LinkView {
				kind: link.kind,
				url: link.url,
			})
			.collect(),
		revision: id_for(&revision_digest),
	};
	Json(view).into_response()
}

#[derive(Serialize)]
struct ReleaseView {
	project_id: String,
	human_version: String,
	channel: String,
	kind: String,
	declared_time: i64,
	license_expression: Option<String>,
	artifacts: Vec<ArtifactView>,
	compatibility: Vec<CompatibilityView>,
	dependencies: Vec<DependencyView>,
	rights: Option<RightsView>,
}

#[derive(Serialize)]
struct ArtifactView {
	digest: String,
	size: u64,
	media_type: String,
	filename: String,
	is_primary: bool,
}

#[derive(Serialize)]
struct CompatibilityView {
	scheme: String,
	values: Vec<String>,
	loader_id: Option<String>,
	side: String,
}

#[derive(Serialize)]
struct DependencyView {
	target_kind: String,
	target_id: String,
	kind: String,
}

#[derive(Serialize)]
struct RightsView {
	redistribution: String,
	modpack_inclusion: String,
	mirroring: String,
	attribution_required: bool,
}

async fn release_view(State(state): State<AppState>, Path((id, hex_digest)): Path<(String, String)>) -> Response {
	let Some(digest) = parse_hex_digest(&hex_digest) else {
		return (StatusCode::BAD_REQUEST, "invalid digest").into_response();
	};
	let Some(object) = (match state.metadata.object(&digest).await {
		Ok(object) => object,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::NOT_FOUND, "no such release").into_response();
	};
	let release = match moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload) {
		Ok(moraine_model::release::ReleaseObject::Release(release)) => release,
		Ok(_) => return (StatusCode::NOT_FOUND, "object is not a release").into_response(),
		Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "stored release does not decode").into_response(),
	};
	if release.project_id != id {
		return (StatusCode::NOT_FOUND, "release does not belong to this project").into_response();
	}
	let view = ReleaseView {
		project_id: release.project_id,
		human_version: release.human_version,
		channel: release.channel,
		kind: release.kind,
		declared_time: release.declared_time,
		license_expression: release.license_expression,
		artifacts: release
			.artifacts
			.into_iter()
			.map(|artifact| ArtifactView {
				digest: format!("sha256:{}", hex::encode(artifact.digest)),
				size: artifact.size,
				media_type: artifact.media_type,
				filename: artifact.filename,
				is_primary: artifact.is_primary,
			})
			.collect(),
		compatibility: release
			.compatibility
			.into_iter()
			.map(|entry| CompatibilityView {
				scheme: entry.game_version_predicate.scheme,
				values: entry.game_version_predicate.values,
				loader_id: entry.loader_id,
				side: entry.side.as_str().to_string(),
			})
			.collect(),
		dependencies: release
			.dependencies
			.into_iter()
			.map(|dependency| DependencyView {
				target_kind: dependency.target_kind.as_str().to_string(),
				target_id: dependency.target_id,
				kind: dependency.kind.as_str().to_string(),
			})
			.collect(),
		rights: release.rights.map(|rights| RightsView {
			redistribution: rights.redistribution.as_str().to_string(),
			modpack_inclusion: rights.modpack_inclusion.as_str().to_string(),
			mirroring: rights.mirroring.as_str().to_string(),
			attribution_required: rights.attribution_required,
		}),
	};
	Json(view).into_response()
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

fn describe_stored(object: &StoredObject) -> Option<String> {
	match object.kind.as_str() {
		"release" => match moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload) {
			Ok(moraine_model::release::ReleaseObject::Release(release)) => {
				Some(format!("{} ({})", release.human_version, release.channel))
			}
			_ => None,
		},
		"profile" => ProfileRevision::from_canonical_bytes(&object.payload)
			.ok()
			.map(|profile| profile.display_name),
		"advisory" => Advisory::from_canonical_bytes(&object.payload)
			.ok()
			.map(|advisory| format!("{} {}", advisory.severity.as_str(), advisory.category.as_str())),
		"delegation" => Delegation::from_canonical_bytes(&object.payload)
			.ok()
			.map(|delegation| delegation.purpose().as_str().to_string()),
		_ => None,
	}
}

pub(crate) fn stored(object: &verify::VerifiedObject) -> StoredObject {
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
