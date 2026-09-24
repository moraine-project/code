use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use moraine_crypto::ObjectKind;
use moraine_model::Canonical;
use moraine_model::delegation::Delegation;
use moraine_model::feed::FeedEntry;
use moraine_model::signed::SignedObject;
use moraine_model::trust::{RootSet, verify_key_delegation};
use serde::Serialize;

use super::projects::{apply_ownership_transfer, apply_withdrawal};
use crate::db::{AppendFeed, FeedRow, ProjectRow, StoredObject};
use crate::registry::{migration, recovery};
use crate::routes::AppState;
use crate::verify::{self, VerifyError};

pub(super) async fn append_feed(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
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

#[derive(Serialize)]
struct FeedReceipt {
	seq: i64,
	entry: String,
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

	let replayed = match state.metadata.feed_at(project_id, entry.sequence as i64).await {
		Ok(Some(stored_entry)) => {
			if stored_entry.entry_digest != object.digest {
				return Err(Box::new(
					(StatusCode::CONFLICT, "a different entry already occupies this sequence").into_response(),
				));
			}
			Some(stored_entry)
		}
		Ok(None) => None,
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	if let Some(stored_entry) = replayed {
		return Ok((stored_entry.seq, id_for(&stored_entry.entry_digest)));
	}

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
	match state.metadata.append_feed(&row).await {
		Ok(AppendFeed::Appended) => {}
		Ok(AppendFeed::HeadMoved) => {
			return Err(Box::new(
				(StatusCode::CONFLICT, "the feed head moved while this entry was being stored").into_response(),
			));
		}
		Err(error) => return Err(Box::new(storage_error(error))),
	}
	if row.kind == "profile-updated"
		&& let Err(error) = crate::registry::search_index::refresh_search_document(state, &row.object_digest).await
	{
		return Err(Box::new(storage_error(error)));
	}
	if row.kind == "ownership-transferred" {
		apply_ownership_transfer(state, &row.project_id, &row.object_digest).await?;
	}
	if row.kind == "release-withdrawn" {
		apply_withdrawal(state, &row.project_id, &row.object_digest).await?;
	}
	if row.kind == "key-changed" {
		state.metrics.record_key_change();
	}
	if row.kind == "recovery" {
		state.metrics.record_key_change();
		recovery::apply(state, &row.project_id, &row.object_digest).await?;
	}
	if row.kind == "migration" {
		migration::apply(state, &row.project_id, &row.object_digest).await?;
	}
	if state.capability.publishing == "progressive"
		&& matches!(row.kind.as_str(), "key-changed" | "recovery" | "ownership-transferred")
		&& let Err(error) = state
			.metadata
			.suspend_publication_grants(&row.project_id, row.kind.as_str(), unix_now())
			.await
	{
		return Err(Box::new(storage_error(error)));
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

pub(crate) async fn load_root(state: &AppState, project_id: &str) -> Result<RootSet, Box<Response>> {
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
	let parsed = match moraine_model::genesis::Genesis::from_canonical_bytes(&genesis.payload) {
		Ok(genesis) => genesis,
		Err(error) => return Err(Box::new(bad_request(state, VerifyError::Decode(error)))),
	};
	match state.metadata.project_roots(project_id).await {
		Ok(Some(row)) => {
			let Some(keys) = crate::registry::recovery::decode_roots(&row.roots) else {
				return Err(Box::new(
					(StatusCode::INTERNAL_SERVER_ERROR, "stored roots do not decode").into_response(),
				));
			};
			let threshold = match usize::try_from(row.threshold) {
				Ok(threshold) => threshold,
				Err(_) => {
					return Err(Box::new(
						(StatusCode::INTERNAL_SERVER_ERROR, "stored threshold is invalid").into_response(),
					));
				}
			};
			RootSet::new(project_id, &keys, threshold, parsed.authorized_kinds.clone(), parsed.kind)
				.map_err(|error| Box::new(bad_request(state, VerifyError::Decode(error))))
		}
		Ok(None) => RootSet::from_genesis(&parsed, project_id)
			.map_err(|error| Box::new(bad_request(state, VerifyError::Decode(error)))),
		Err(error) => Err(Box::new(storage_error(error))),
	}
}

pub(crate) async fn load_delegations(
	state: &AppState,
	project_id: &str,
	root: &RootSet,
) -> Result<Vec<SignedObject<Delegation>>, Box<Response>> {
	let stored = match state.metadata.project_delegations(project_id, 500).await {
		Ok(stored) => stored,
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	let mut delegations = Vec::new();
	for object in stored {
		let Ok(signed) = SignedObject::<Delegation>::from_bytes(&object.wire) else {
			continue;
		};
		if !matches!(signed.payload, Delegation::Key(_)) {
			continue;
		}
		if verify_key_delegation(&signed, root).is_err() {
			continue;
		}
		delegations.push(signed);
	}
	Ok(delegations)
}

pub(crate) fn unix_now() -> i64 {
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
	if object.kind == ObjectKind::Delegation
		&& let Ok(moraine_model::delegation::Delegation::Key(key)) =
			moraine_model::delegation::Delegation::from_canonical_bytes(&object.payload_bytes)
	{
		state
			.metadata
			.index_project_delegation(&key.project_id, &object.digest)
			.await?;
	}
	if object.kind == ObjectKind::Profile
		&& let Ok(profile) = moraine_model::profile::ProfileRevision::from_canonical_bytes(&object.payload_bytes)
		&& let Some(game) = crate::registry::search_index::game_vocabulary(state, &profile.game_id).await?
		&& let Err(message) = crate::registry::search_index::validate_profile_vocabulary(&profile, &game)
	{
		return Err(sqlx::Error::Protocol(message));
	}
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
				crate::registry::search_labels::index_release_labels(state, &release).await?;
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
	if object.kind == ObjectKind::Attestation
		&& let Ok(moraine_model::attestation::AttestationObject::Evidence(attestation)) =
			moraine_model::attestation::AttestationObject::from_canonical_bytes(&object.payload_bytes)
	{
		state.metadata.insert_attestation(&attestation, &object.digest).await?;
	}
	if object.kind == ObjectKind::Delegation
		&& let Ok(signed) = SignedObject::<Delegation>::from_bytes(&object.wire_bytes)
		&& let Delegation::Recovery(event) = &signed.payload
	{
		state
			.metadata
			.note_recovery_claim(
				&event.project_id,
				&object.digest,
				i64::try_from(event.valid_from_seq).unwrap_or(i64::MAX),
				&recovery::encode_roots(&event.replacement_roots),
				unix_now(),
			)
			.await?;
	}
	if object.kind == ObjectKind::Changelog
		&& let Ok(changelog) = moraine_model::changelog::Changelog::from_canonical_bytes(&object.payload_bytes)
	{
		state
			.metadata
			.put_changelog_text(&object.digest, &changelog.project_id, &changelog.text())
			.await?;
	}
	Ok(())
}

pub(crate) fn bad_request(state: &AppState, error: VerifyError) -> Response {
	if matches!(error, VerifyError::Signature(_)) {
		state.metrics.record_signature_failure();
	}
	(StatusCode::BAD_REQUEST, error.to_string()).into_response()
}

pub(crate) fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "metadata store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
