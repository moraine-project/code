use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_crypto::{ObjectKind, object_id};
use moraine_model::advisory::Advisory;
use moraine_model::signed::{SignedObject, TrustedKey, verify_envelope};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::auth::AuthenticatedUser;
use crate::routes::AppState;
use crate::store::{MetadataStore, StoredObject};

#[derive(Debug, Clone)]
pub struct AdvisoryRow {
	pub digest: Vec<u8>,
	pub provider_id: String,
	pub project_id: String,
	pub game_id: String,
	pub affected_digest: Option<Vec<u8>>,
	pub severity: String,
	pub category: String,
	pub block_promotion: bool,
	pub published_at: i64,
	pub retracted_at: Option<i64>,
}

impl MetadataStore {
	pub async fn pin_provider(
		&self,
		provider_id: &str,
		public_key: &[u8],
		added_by: &str,
		added_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO providers (provider_id, public_key, added_by, added_at) VALUES ($1, $2, $3, $4)
			 ON CONFLICT(provider_id) DO UPDATE SET public_key = $2, added_by = $3, added_at = $4",
		)
		.bind(provider_id)
		.bind(public_key)
		.bind(added_by)
		.bind(added_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn provider(&self, provider_id: &str) -> Result<Option<Vec<u8>>, sqlx::Error> {
		let row = sqlx::query("SELECT public_key FROM providers WHERE provider_id = $1")
			.bind(provider_id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.map(|row| row.get("public_key")))
	}

	pub async fn insert_advisory(&self, advisory: &AdvisoryRow) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO advisories
			 (digest, provider_id, project_id, game_id, affected_digest, severity, category, block_promotion, published_at, retracted_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
			 ON CONFLICT (digest) DO UPDATE SET provider_id = $2, project_id = $3, game_id = $4,
			 affected_digest = $5, severity = $6, category = $7, block_promotion = $8,
			 published_at = $9, retracted_at = $10",
		)
		.bind(&advisory.digest)
		.bind(&advisory.provider_id)
		.bind(&advisory.project_id)
		.bind(&advisory.game_id)
		.bind(&advisory.affected_digest)
		.bind(&advisory.severity)
		.bind(&advisory.category)
		.bind(i64::from(advisory.block_promotion))
		.bind(advisory.published_at)
		.bind(advisory.retracted_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn advisories_for_digest(&self, digest: &[u8]) -> Result<Vec<AdvisoryRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT digest, provider_id, project_id, game_id, affected_digest, severity, category, block_promotion, published_at, retracted_at
			 FROM advisories WHERE affected_digest = $1 ORDER BY published_at DESC",
		)
		.bind(digest)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(advisory_from_row).collect())
	}

	pub async fn advisories_for_project(&self, project_id: &str) -> Result<Vec<AdvisoryRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT digest, provider_id, project_id, game_id, affected_digest, severity, category, block_promotion, published_at, retracted_at
			 FROM advisories WHERE project_id = $1 ORDER BY published_at DESC",
		)
		.bind(project_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(advisory_from_row).collect())
	}
}

fn advisory_from_row(row: sqlx::any::AnyRow) -> AdvisoryRow {
	AdvisoryRow {
		digest: row.get("digest"),
		provider_id: row.get("provider_id"),
		project_id: row.get("project_id"),
		game_id: row.get("game_id"),
		affected_digest: row.get("affected_digest"),
		severity: row.get("severity"),
		category: row.get("category"),
		block_promotion: row.get::<i64, _>("block_promotion") != 0,
		published_at: row.get("published_at"),
		retracted_at: row.get("retracted_at"),
	}
}

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/providers/{provider_id}/keys", post(pin_provider))
		.route("/v1/advisories", get(list_advisories).post(publish_advisory))
}

#[derive(Deserialize)]
struct ProviderKey {
	public_key: String,
}

async fn pin_provider(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	axum::extract::Path(provider_id): axum::extract::Path<String>,
	Json(request): Json<ProviderKey>,
) -> Response {
	let provider_id = provider_id.trim().to_string();
	if provider_id.is_empty() || provider_id.len() > 128 {
		return (StatusCode::BAD_REQUEST, "invalid provider id").into_response();
	}
	let public_key = match hex::decode(request.public_key.trim()) {
		Ok(bytes) => bytes,
		Err(_) => return (StatusCode::BAD_REQUEST, "public_key must be hex").into_response(),
	};
	if let Err(error) = TrustedKey::new(&public_key) {
		return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
	}
	if let Ok(Some(existing)) = state.metadata.provider(&provider_id).await
		&& existing != public_key
	{
		return (StatusCode::CONFLICT, "provider already pinned with a different key").into_response();
	}
	match state
		.metadata
		.pin_provider(&provider_id, &public_key, &user.user_id, now())
		.await
	{
		Ok(()) => (StatusCode::CREATED, Json(serde_json::json!({ "provider_id": provider_id }))).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn publish_advisory(State(state): State<AppState>, body: Bytes) -> Response {
	let signed = match SignedObject::<Advisory>::from_bytes(&body) {
		Ok(signed) => signed,
		Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	};
	let advisory = &signed.payload;
	let Some(public_key) = (match state.metadata.provider(&advisory.provider_id).await {
		Ok(provider) => provider,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::CONFLICT, "provider key is not pinned on this instance").into_response();
	};
	let trusted = match TrustedKey::new(&public_key) {
		Ok(trusted) => trusted,
		Err(error) => return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
	};
	if verify_envelope(&signed.envelope, &signed.signed_message(ObjectKind::Advisory), &[trusted], 1).is_err() {
		return (
			StatusCode::BAD_REQUEST,
			"advisory signature did not verify against the pinned key",
		)
			.into_response();
	}
	let digest = object_id(ObjectKind::Advisory, &signed.payload_bytes);
	let stored = StoredObject {
		digest: digest.to_vec(),
		kind: "advisory".to_string(),
		payload: signed.payload_bytes.clone(),
		wire: body.to_vec(),
	};
	if let Err(error) = state.metadata.put_object(&stored).await {
		return storage_error(error);
	}
	let row = AdvisoryRow {
		digest: digest.to_vec(),
		provider_id: advisory.provider_id.clone(),
		project_id: advisory.project_id.clone(),
		game_id: advisory.game_id.clone(),
		affected_digest: advisory.affected.digest.clone(),
		severity: advisory.severity.as_str().to_string(),
		category: advisory.category.as_str().to_string(),
		block_promotion: advisory.block_promotion,
		published_at: advisory.published_at,
		retracted_at: advisory.retracted_at,
	};
	if let Err(error) = state.metadata.insert_advisory(&row).await {
		return storage_error(error);
	}
	(StatusCode::CREATED, Json(serde_json::json!({ "advisory": id_for(&digest) }))).into_response()
}

#[derive(Deserialize)]
struct AdvisoryQuery {
	#[serde(default)]
	project: Option<String>,
	#[serde(default)]
	digest: Option<String>,
}

async fn list_advisories(State(state): State<AppState>, Query(query): Query<AdvisoryQuery>) -> Response {
	let rows = if let Some(digest) = query.digest.as_deref() {
		let hex = digest.strip_prefix("sha256:").unwrap_or(digest);
		let Some(bytes) = hex::decode(hex).ok().filter(|bytes| bytes.len() == 32) else {
			return (StatusCode::BAD_REQUEST, "digest must be a sha256 digest").into_response();
		};
		state.metadata.advisories_for_digest(&bytes).await
	} else if let Some(project) = query.project.as_deref() {
		state.metadata.advisories_for_project(project).await
	} else {
		return (StatusCode::BAD_REQUEST, "pass project or digest").into_response();
	};
	match rows {
		Ok(rows) => Json(rows.into_iter().map(advisory_view).collect::<Vec<_>>()).into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Serialize)]
pub struct AdvisoryView {
	pub advisory: String,
	pub provider_id: String,
	pub project_id: String,
	pub severity: String,
	pub category: String,
	pub block_promotion: bool,
	pub affected_digest: Option<String>,
	pub published_at: i64,
	pub retracted_at: Option<i64>,
}

pub fn advisory_view(row: AdvisoryRow) -> AdvisoryView {
	AdvisoryView {
		advisory: id_for(&row.digest),
		provider_id: row.provider_id,
		project_id: row.project_id,
		severity: row.severity,
		category: row.category,
		block_promotion: row.block_promotion,
		affected_digest: row.affected_digest.map(|digest| format!("sha256:{}", hex::encode(digest))),
		published_at: row.published_at,
		retracted_at: row.retracted_at,
	}
}

fn id_for(digest: &[u8]) -> String {
	format!("gd:sha256:{}", hex::encode(digest))
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "advisory store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
