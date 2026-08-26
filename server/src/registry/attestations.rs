use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_crypto::{ObjectKind, object_id};
use moraine_model::Canonical;
use moraine_model::attestation::{Attestation, AttestationKind, AttestationObject};
use moraine_model::signed::{SignedObject, TrustedKey, verify_envelope};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::db::{MetadataStore, StoredObject};
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/attestations", axum::routing::post(publish_attestation))
		.route("/v1/attestations/{digest}", get(list_attestations))
}

impl MetadataStore {
	pub async fn insert_attestation(&self, attestation: &Attestation, object_digest: &[u8]) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO evidence_attestations (object_digest, artifact_digest, kind, signer_id, subject_kind, subject_id,
			 media_type, issued_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
			 ON CONFLICT(object_digest) DO NOTHING",
		)
		.bind(object_digest)
		.bind(&attestation.artifact_digest)
		.bind(attestation.kind.as_str())
		.bind(&attestation.signer_id)
		.bind(&attestation.subject_kind)
		.bind(&attestation.subject_id)
		.bind(&attestation.media_type)
		.bind(attestation.issued_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn attestations_for(&self, artifact_digest: &[u8], kind: Option<&str>) -> Result<Vec<Vec<u8>>, sqlx::Error> {
		let rows = match kind {
			Some(kind) => {
				sqlx::query(
					"SELECT object_digest FROM evidence_attestations WHERE artifact_digest = $1 AND kind = $2
					 ORDER BY issued_at DESC, object_digest ASC",
				)
				.bind(artifact_digest)
				.bind(kind)
				.fetch_all(&self.pool)
				.await?
			}
			None => {
				sqlx::query(
					"SELECT object_digest FROM evidence_attestations WHERE artifact_digest = $1
					 ORDER BY issued_at DESC, object_digest ASC",
				)
				.bind(artifact_digest)
				.fetch_all(&self.pool)
				.await?
			}
		};
		Ok(rows.into_iter().map(|row| row.get("object_digest")).collect())
	}
}

#[derive(Serialize)]
struct AttestationView {
	attestation: String,
	kind: String,
	signer_id: String,
	subject_kind: String,
	subject_id: String,
	media_type: String,
	issued_at: i64,
	body_digest: Option<String>,
	has_inline_body: bool,
}

async fn list_attestations(
	State(state): State<AppState>,
	Path(digest): Path<String>,
	Query(query): Query<KindQuery>,
) -> Response {
	let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
	let Some(bytes) = hex::decode(hex).ok().filter(|bytes| bytes.len() == 32) else {
		return (StatusCode::BAD_REQUEST, "digest must be a sha256 digest").into_response();
	};
	let mut kind = None;
	if let Some(requested) = query.kind.as_deref() {
		let Some(parsed) = AttestationKind::parse(requested) else {
			return (StatusCode::BAD_REQUEST, format!("unknown attestation kind `{requested}`")).into_response();
		};
		kind = Some(parsed.as_str());
	}
	let rows = match state.metadata.attestations_for(&bytes, kind).await {
		Ok(rows) => rows,
		Err(error) => return storage_error(error),
	};
	let mut attestations = Vec::with_capacity(rows.len());
	for object_digest in rows {
		let Some(object) = (match state.metadata.object(&object_digest).await {
			Ok(object) => object,
			Err(error) => return storage_error(error),
		}) else {
			continue;
		};
		let Ok(AttestationObject::Evidence(attestation)) = AttestationObject::from_canonical_bytes(&object.payload) else {
			continue;
		};
		attestations.push(AttestationView {
			attestation: id_for(&object_digest),
			kind: attestation.kind.as_str().to_string(),
			signer_id: attestation.signer_id,
			subject_kind: attestation.subject_kind,
			subject_id: attestation.subject_id,
			media_type: attestation.media_type,
			issued_at: attestation.issued_at,
			body_digest: attestation
				.body_digest
				.map(|digest| format!("sha256:{}", hex::encode(digest))),
			has_inline_body: attestation.body_inline.is_some(),
		});
	}
	Json(attestations).into_response()
}

#[derive(Deserialize)]
struct KindQuery {
	kind: Option<String>,
}

async fn publish_attestation(State(state): State<AppState>, body: Bytes) -> Response {
	let signed = match SignedObject::<AttestationObject>::from_bytes(&body) {
		Ok(signed) => signed,
		Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	};
	let AttestationObject::Evidence(attestation) = &signed.payload else {
		return (StatusCode::BAD_REQUEST, "object is not an evidence attestation").into_response();
	};
	let Some(public_key) = (match state.metadata.provider(&attestation.signer_id).await {
		Ok(provider) => provider,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::CONFLICT, "signer key is not pinned on this instance").into_response();
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
			"attestation signature did not verify against the pinned key",
		)
			.into_response();
	}
	let digest = object_id(ObjectKind::Attestation, &signed.payload_bytes);
	let stored = StoredObject {
		digest: digest.to_vec(),
		kind: "attestation".to_string(),
		payload: signed.payload_bytes.clone(),
		wire: body.to_vec(),
	};
	if let Err(error) = state.metadata.put_object(&stored).await {
		return storage_error(error);
	}
	if let Err(error) = state.metadata.insert_attestation(attestation, &digest).await {
		return storage_error(error);
	}
	(
		StatusCode::CREATED,
		Json(serde_json::json!({ "attestation": id_for(&digest) })),
	)
		.into_response()
}

fn id_for(digest: &[u8]) -> String {
	format!("gd:sha256:{}", hex::encode(digest))
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "attestation store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
