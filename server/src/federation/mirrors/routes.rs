use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_crypto::{ObjectKind, object_id};
use moraine_model::attestation::AttestationObject;
use moraine_model::signed::{SignedObject, TrustedKey, verify_envelope};
use serde::{Deserialize, Serialize};

use super::store::CommitmentRow;
use super::{id_for, now};
use crate::auth::AuthenticatedUser;
use crate::db::StoredObject;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/mirrors/{mirror_id}/keys", post(pin_mirror))
		.route("/v1/mirror-commitments", post(publish_commitment))
		.route("/v1/artifacts/sha256/{digest}/locations", get(artifact_locations_for))
		.route("/v1/mirrors/{digest}", get(mirrors_for))
}

#[derive(Deserialize)]
struct MirrorKey {
	public_key: String,
}

async fn pin_mirror(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Path(mirror_id): Path<String>,
	Json(request): Json<MirrorKey>,
) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let mirror_id = mirror_id.trim().to_string();
	if mirror_id.is_empty() || mirror_id.len() > 128 {
		return (StatusCode::BAD_REQUEST, "invalid mirror id").into_response();
	}
	let public_key = match hex::decode(request.public_key.trim()) {
		Ok(bytes) => bytes,
		Err(_) => return (StatusCode::BAD_REQUEST, "public_key must be hex").into_response(),
	};
	if let Err(error) = TrustedKey::new(&public_key) {
		return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
	}
	if let Ok(Some(existing)) = state.metadata.mirror_key(&mirror_id).await
		&& existing != public_key
	{
		return (StatusCode::CONFLICT, "mirror already pinned with a different key").into_response();
	}
	match state.metadata.pin_mirror(&mirror_id, &public_key, &user.user_id, now()).await {
		Ok(()) => (StatusCode::CREATED, Json(serde_json::json!({ "mirror_id": mirror_id }))).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn publish_commitment(State(state): State<AppState>, body: Bytes) -> Response {
	let signed = match SignedObject::<AttestationObject>::from_bytes(&body) {
		Ok(signed) => signed,
		Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	};
	let AttestationObject::MirrorCommitment(commitment) = &signed.payload else {
		return (StatusCode::BAD_REQUEST, "object is not a mirror commitment").into_response();
	};
	let Some(public_key) = (match state.metadata.mirror_key(&commitment.mirror_id).await {
		Ok(key) => key,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::CONFLICT, "mirror key is not pinned on this instance").into_response();
	};
	let trusted = match TrustedKey::new(&public_key) {
		Ok(trusted) => trusted,
		Err(error) => return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
	};
	if verify_envelope(
		&signed.envelope,
		&signed.signed_message(ObjectKind::Attestation),
		&[trusted],
		1,
	)
	.is_err()
	{
		return (
			StatusCode::BAD_REQUEST,
			"commitment signature did not verify against the pinned key",
		)
			.into_response();
	}
	let object_digest = object_id(ObjectKind::Attestation, &signed.payload_bytes);
	let stored = StoredObject {
		digest: object_digest.to_vec(),
		kind: "attestation".to_string(),
		payload: signed.payload_bytes.clone(),
		wire: body.to_vec(),
	};
	if let Err(error) = state.metadata.put_object(&stored).await {
		return storage_error(error);
	}
	let row = CommitmentRow {
		mirror_id: commitment.mirror_id.clone(),
		size: commitment.size as i64,
		accepted_at: commitment.accepted_at,
		retention_until: commitment.retention_until,
		endpoint: commitment.endpoint.clone(),
		object_digest: object_digest.to_vec(),
	};
	match state
		.metadata
		.insert_commitment(&row, &commitment.artifact_digest, &object_digest)
		.await
	{
		Ok(()) => (
			StatusCode::CREATED,
			Json(serde_json::json!({ "commitment": id_for(&object_digest) })),
		)
			.into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Serialize)]
struct MirrorView {
	digest: String,
	locations: Vec<LocationView>,
	commitments: Vec<CommitmentView>,
}

#[derive(Serialize)]
struct ArtifactLocationsView {
	protocol: u32,
	algorithm: &'static str,
	digest: String,
	size: Option<i64>,
	locations: Vec<ArtifactLocationView>,
	refreshed_at: i64,
}

#[derive(Serialize)]
struct ArtifactLocationView {
	url: String,
	kind: String,
	provenance: &'static str,
	operator_id: Option<String>,
	location_record_digest: Option<String>,
	commitment_digest: Option<String>,
	supports_ranges: Option<bool>,
	last_success_at: Option<i64>,
	expires_at: Option<i64>,
	priority: Option<i64>,
}

#[derive(Serialize)]
struct LocationView {
	url: String,
	kind: String,
	operator_id: Option<String>,
}

#[derive(Serialize)]
struct CommitmentView {
	mirror_id: String,
	size: i64,
	accepted_at: i64,
	retention_until: Option<i64>,
	endpoint: String,
	last_checked_at: Option<i64>,
	reachable: Option<bool>,
}

async fn mirrors_for(State(state): State<AppState>, Path(digest): Path<String>) -> Response {
	let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
	let Some(bytes) = hex::decode(hex).ok().filter(|bytes| bytes.len() == 32) else {
		return (StatusCode::BAD_REQUEST, "digest must be a sha256 digest").into_response();
	};
	let locations = match state.metadata.locations_for(&bytes).await {
		Ok(locations) => locations,
		Err(error) => return storage_error(error),
	};
	let commitments = match state.metadata.commitments_for(&bytes).await {
		Ok(commitments) => commitments,
		Err(error) => return storage_error(error),
	};
	let mut confirmations = Vec::with_capacity(commitments.len());
	for commitment in &commitments {
		match state.metadata.confirmation_for(&bytes, &commitment.mirror_id).await {
			Ok(confirmation) => confirmations.push(confirmation),
			Err(error) => return storage_error(error),
		}
	}
	let view = MirrorView {
		digest: format!("sha256:{}", hex::encode(&bytes)),
		locations: locations
			.into_iter()
			.map(|location| LocationView {
				url: location.url,
				kind: location.kind,
				operator_id: location.operator_id,
			})
			.collect(),
		commitments: commitments
			.into_iter()
			.zip(confirmations)
			.map(|(commitment, confirmation)| CommitmentView {
				mirror_id: commitment.mirror_id,
				size: commitment.size,
				accepted_at: commitment.accepted_at,
				retention_until: commitment.retention_until,
				endpoint: commitment.endpoint,
				last_checked_at: confirmation.map(|confirmation| confirmation.checked_at),
				reachable: confirmation.map(|confirmation| confirmation.reachable),
			})
			.collect(),
	};
	Json(view).into_response()
}

async fn artifact_locations_for(State(state): State<AppState>, Path(digest): Path<String>) -> Response {
	let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
	let Some(bytes) = hex::decode(hex).ok().filter(|bytes| bytes.len() == 32) else {
		return (StatusCode::BAD_REQUEST, "digest must be a sha256 digest").into_response();
	};
	let locations = match state.metadata.locations_for(&bytes).await {
		Ok(locations) => locations,
		Err(error) => return storage_error(error),
	};
	let commitments = match state.metadata.commitments_for(&bytes).await {
		Ok(commitments) => commitments,
		Err(error) => return storage_error(error),
	};
	let mut views = locations
		.into_iter()
		.map(|location| ArtifactLocationView {
			url: location.url,
			kind: location.kind,
			provenance: "publisher-authorized",
			operator_id: location.operator_id,
			location_record_digest: Some(id_for(&location.object_digest)),
			commitment_digest: None,
			supports_ranges: None,
			last_success_at: None,
			expires_at: None,
			priority: None,
		})
		.collect::<Vec<_>>();
	let size = commitments.first().map(|commitment| commitment.size);
	for commitment in commitments {
		let confirmation = match state.metadata.confirmation_for(&bytes, &commitment.mirror_id).await {
			Ok(confirmation) => confirmation,
			Err(error) => return storage_error(error),
		};
		views.push(ArtifactLocationView {
			url: commitment.endpoint,
			kind: "mirror".to_string(),
			provenance: "mirror-committed",
			operator_id: Some(commitment.mirror_id),
			location_record_digest: None,
			commitment_digest: Some(id_for(&commitment.object_digest)),
			supports_ranges: None,
			last_success_at: confirmation.and_then(|value| value.reachable.then_some(value.checked_at)),
			expires_at: commitment.retention_until,
			priority: None,
		});
	}
	Json(ArtifactLocationsView {
		protocol: 1,
		algorithm: "sha256",
		digest: format!("sha256:{}", hex::encode(&bytes)),
		size,
		locations: views,
		refreshed_at: now(),
	})
	.into_response()
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "mirror store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
