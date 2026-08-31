use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_crypto::ObjectKind;
use moraine_model::Canonical;
use moraine_model::delegation::Delegation;
use moraine_model::genesis::{Genesis, RootKey};
use moraine_model::signed::{SignedObject, verify_envelope};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::db::MetadataStore;
use crate::routes::AppState;

pub(crate) fn routes() -> Router<AppState> {
	Router::new().route("/v1/projects/{id}/recovery", get(recovery_view))
}

#[derive(Debug, Clone)]
pub struct ProjectRootsRow {
	pub roots: String,
	pub threshold: i64,
	pub valid_from_seq: i64,
}

#[derive(Debug, Clone)]
pub struct RecoveryClaimRow {
	pub object_digest: Vec<u8>,
	pub valid_from_seq: i64,
	pub roots: String,
	pub applied: bool,
}

impl MetadataStore {
	pub async fn project_roots(&self, project_id: &str) -> Result<Option<ProjectRootsRow>, sqlx::Error> {
		let row = sqlx::query("SELECT roots, threshold, valid_from_seq FROM project_roots WHERE project_id = $1")
			.bind(project_id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.map(|row| ProjectRootsRow {
			roots: row.get("roots"),
			threshold: row.get("threshold"),
			valid_from_seq: row.get("valid_from_seq"),
		}))
	}

	pub async fn set_project_roots(
		&self,
		project_id: &str,
		roots: &str,
		threshold: i64,
		valid_from_seq: i64,
		updated_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO project_roots (project_id, roots, threshold, valid_from_seq, updated_at)
			 VALUES ($1, $2, $3, $4, $5)
			 ON CONFLICT(project_id) DO UPDATE SET roots = $2, threshold = $3, valid_from_seq = $4, updated_at = $5",
		)
		.bind(project_id)
		.bind(roots)
		.bind(threshold)
		.bind(valid_from_seq)
		.bind(updated_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn note_recovery_claim(
		&self,
		project_id: &str,
		object_digest: &[u8],
		valid_from_seq: i64,
		roots: &str,
		recorded_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO recovery_claims (project_id, object_digest, valid_from_seq, roots, applied, recorded_at)
			 VALUES ($1, $2, $3, $4, 0, $5)
			 ON CONFLICT(project_id, object_digest) DO NOTHING",
		)
		.bind(project_id)
		.bind(object_digest)
		.bind(valid_from_seq)
		.bind(roots)
		.bind(recorded_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn mark_recovery_claim_applied(
		&self,
		project_id: &str,
		object_digest: &[u8],
		valid_from_seq: i64,
		roots: &str,
		recorded_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO recovery_claims (project_id, object_digest, valid_from_seq, roots, applied, recorded_at)
			 VALUES ($1, $2, $3, $4, 1, $5)
			 ON CONFLICT(project_id, object_digest) DO UPDATE SET applied = 1, roots = $4",
		)
		.bind(project_id)
		.bind(object_digest)
		.bind(valid_from_seq)
		.bind(roots)
		.bind(recorded_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn recovery_claims_for(&self, project_id: &str) -> Result<Vec<RecoveryClaimRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT object_digest, valid_from_seq, roots, applied FROM recovery_claims WHERE project_id = $1
			 ORDER BY valid_from_seq DESC, recorded_at ASC",
		)
		.bind(project_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| RecoveryClaimRow {
				object_digest: row.get("object_digest"),
				valid_from_seq: row.get("valid_from_seq"),
				roots: row.get("roots"),
				applied: row.get::<i64, _>("applied") != 0,
			})
			.collect())
	}
}

#[derive(Serialize, Deserialize)]
struct RootJson {
	key_id: String,
	public_key: String,
}

pub(crate) fn encode_roots(roots: &[RootKey]) -> String {
	let encoded: Vec<RootJson> = roots
		.iter()
		.map(|root| RootJson {
			key_id: root.key_id.to_string(),
			public_key: hex::encode(&root.public_key),
		})
		.collect();
	serde_json::to_string(&encoded).unwrap_or_else(|_| "[]".to_string())
}

pub(crate) fn decode_roots(text: &str) -> Option<Vec<Vec<u8>>> {
	let decoded: Vec<RootJson> = serde_json::from_str(text).ok()?;
	decoded.into_iter().map(|root| hex::decode(root.public_key).ok()).collect()
}

pub(crate) async fn apply(state: &AppState, project_id: &str, object_digest: &[u8]) -> Result<(), Box<Response>> {
	let Some(object) = (match state.metadata.object(object_digest).await {
		Ok(object) => object,
		Err(error) => return Err(Box::new(super::storage_error(error))),
	}) else {
		return Ok(());
	};
	let Ok(signed) = SignedObject::<Delegation>::from_bytes(&object.wire) else {
		return Ok(());
	};
	let Delegation::Recovery(event) = &signed.payload else {
		return Ok(());
	};
	let root = super::load_root(state, project_id).await?;
	let threshold = root.threshold();
	let message = signed.signed_message(ObjectKind::Delegation);
	let valid = match verify_envelope(&signed.envelope, &message, root.keys(), threshold) {
		Ok(valid) => valid,
		Err(error) => {
			return Err(Box::new(
				(StatusCode::BAD_REQUEST, format!("recovery did not verify: {error}")).into_response(),
			));
		}
	};
	if valid < threshold {
		return Err(Box::new(
			(StatusCode::BAD_REQUEST, "recovery needs the threshold of recovery keys").into_response(),
		));
	}
	if event.replacement_roots.len() < threshold {
		return Err(Box::new(
			(
				StatusCode::BAD_REQUEST,
				"the replacement root set does not meet the threshold".to_string(),
			)
				.into_response(),
		));
	}
	let applied_seq = match state.metadata.project_roots(project_id).await {
		Ok(roots) => roots.map(|row| row.valid_from_seq).unwrap_or(0),
		Err(error) => return Err(Box::new(super::storage_error(error))),
	};
	let sequence = i64::try_from(event.valid_from_seq).unwrap_or(i64::MAX);
	let roots = encode_roots(&event.replacement_roots);
	if sequence <= applied_seq {
		return Ok(());
	}
	if let Err(error) = state
		.metadata
		.set_project_roots(project_id, &roots, threshold as i64, sequence, now())
		.await
	{
		return Err(Box::new(super::storage_error(error)));
	}
	if let Err(error) = state
		.metadata
		.mark_recovery_claim_applied(project_id, object_digest, sequence, &roots, now())
		.await
	{
		return Err(Box::new(super::storage_error(error)));
	}
	Ok(())
}

#[derive(Serialize)]
struct RootView {
	key_id: String,
	public_key: String,
}

#[derive(Serialize)]
struct ClaimView {
	claim: String,
	valid_from_seq: i64,
	applied: bool,
	roots: Vec<RootView>,
}

#[derive(Serialize)]
struct RecoveryView {
	project_id: String,
	threshold: i64,
	roots: Vec<RootView>,
	valid_from_seq: Option<i64>,
	recovered: bool,
	claims: Vec<ClaimView>,
}

async fn recovery_view(State(state): State<AppState>, Path(id): Path<String>) -> Response {
	let project = match state.metadata.project(&id).await {
		Ok(Some(project)) => project,
		Ok(None) => return (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => return super::storage_error(error),
	};
	let Some(object) = (match state.metadata.object(&project.genesis_digest).await {
		Ok(object) => object,
		Err(error) => return super::storage_error(error),
	}) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "project genesis is missing").into_response();
	};
	let Ok(genesis) = Genesis::from_canonical_bytes(&object.payload) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "stored genesis does not decode").into_response();
	};
	let (roots, threshold, valid_from_seq) = match state.metadata.project_roots(&id).await {
		Ok(Some(row)) => (
			decode_roots(&row.roots).unwrap_or_default(),
			row.threshold,
			Some(row.valid_from_seq),
		),
		Ok(None) => (
			genesis.roots.iter().map(|root| root.public_key.clone()).collect(),
			i64::from(genesis.threshold),
			None,
		),
		Err(error) => return super::storage_error(error),
	};
	let claims = match state.metadata.recovery_claims_for(&id).await {
		Ok(claims) => claims,
		Err(error) => return super::storage_error(error),
	};
	let view = RecoveryView {
		project_id: id,
		threshold,
		roots: roots.iter().map(|key| root_view(key)).collect(),
		valid_from_seq,
		recovered: valid_from_seq.is_some(),
		claims: claims
			.into_iter()
			.map(|claim| ClaimView {
				claim: super::id_for(&claim.object_digest),
				valid_from_seq: claim.valid_from_seq,
				applied: claim.applied,
				roots: decode_roots(&claim.roots)
					.unwrap_or_default()
					.iter()
					.map(|key| root_view(key))
					.collect(),
			})
			.collect(),
	};
	Json(view).into_response()
}

fn root_view(public_key: &[u8]) -> RootView {
	RootView {
		key_id: moraine_crypto::key_id(moraine_crypto::ALG_ED25519, public_key)
			.map(|key_id| key_id.to_string())
			.unwrap_or_default(),
		public_key: hex::encode(public_key),
	}
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}
