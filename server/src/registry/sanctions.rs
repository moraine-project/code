use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::moderation::{REASON_TAXONOMY_VERSION, Sanction, SanctionKind, ScopeKind, ScopeRef};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::auth::AuthenticatedUser;
use crate::db::MetadataStore;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/sanctions", get(list_sanctions).post(record_sanction))
		.route("/v1/sanctions/{id}", get(get_sanction))
}

#[derive(Debug, Clone)]
pub struct SanctionRow {
	pub id: String,
	pub sanction: Sanction,
	pub decided_by: String,
	pub recorded_at: i64,
}

impl MetadataStore {
	pub async fn record_sanction(
		&self,
		id: &str,
		sanction: &Sanction,
		decided_by: &str,
		recorded_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO sanctions (id, subject_user_id, org_id, kind, reason_code, reason_taxonomy_version, scope_kind,
			 scope_id, starts_at, expires_at, decided_by, recorded_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
		)
		.bind(id)
		.bind(&sanction.subject_user_id)
		.bind(sanction.org_id.as_deref())
		.bind(sanction.kind.as_str())
		.bind(&sanction.reason_code)
		.bind(i64::from(sanction.reason_taxonomy_version))
		.bind(sanction.scope.kind.as_str())
		.bind(&sanction.scope.id)
		.bind(sanction.starts_at)
		.bind(sanction.expires_at)
		.bind(decided_by)
		.bind(recorded_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn sanction(&self, id: &str) -> Result<Option<SanctionRow>, sqlx::Error> {
		let row = sqlx::query(sqlx::AssertSqlSafe(format!("{SANCTION_SELECT} WHERE id = $1")))
			.bind(id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.and_then(sanction_row))
	}

	pub async fn sanctions_for(&self, user_id: Option<&str>) -> Result<Vec<SanctionRow>, sqlx::Error> {
		let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
			"{SANCTION_SELECT} WHERE ($1 IS NULL OR subject_user_id = $1) ORDER BY starts_at DESC, id ASC"
		)))
		.bind(user_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().filter_map(sanction_row).collect())
	}

	pub async fn active_publishing_sanction(&self, user_id: &str, now: i64) -> Result<Option<SanctionRow>, sqlx::Error> {
		let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
			"{SANCTION_SELECT} WHERE subject_user_id = $1 AND kind IN ('upload-restriction', 'suspension')
			 AND scope_kind IN ('account', 'instance') AND starts_at <= $2
			 AND (expires_at IS NULL OR expires_at > $2) ORDER BY starts_at DESC, id ASC"
		)))
		.bind(user_id)
		.bind(now)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().filter_map(sanction_row).next())
	}
}

const SANCTION_SELECT: &str = "SELECT id, subject_user_id, org_id, kind, reason_code, reason_taxonomy_version, scope_kind, scope_id, starts_at, expires_at, decided_by, recorded_at FROM sanctions";

fn sanction_row(row: sqlx::any::AnyRow) -> Option<SanctionRow> {
	let kind: String = row.get("kind");
	let scope_kind: String = row.get("scope_kind");
	let taxonomy_version: i64 = row.get("reason_taxonomy_version");
	Some(SanctionRow {
		id: row.get("id"),
		sanction: Sanction {
			subject_user_id: row.get("subject_user_id"),
			org_id: row.get("org_id"),
			kind: SanctionKind::parse(&kind)?,
			reason_code: row.get("reason_code"),
			reason_taxonomy_version: u32::try_from(taxonomy_version).ok()?,
			scope: ScopeRef {
				kind: ScopeKind::parse(&scope_kind)?,
				id: row.get("scope_id"),
			},
			starts_at: row.get("starts_at"),
			expires_at: row.get("expires_at"),
			decided_by: row.get("decided_by"),
		},
		decided_by: row.get("decided_by"),
		recorded_at: row.get("recorded_at"),
	})
}

#[derive(Serialize)]
struct SanctionView {
	id: String,
	subject_user_id: String,
	org_id: Option<String>,
	kind: String,
	reason_code: String,
	reason_taxonomy_version: u32,
	scope_kind: String,
	scope_id: String,
	starts_at: i64,
	expires_at: Option<i64>,
	decided_by: String,
	recorded_at: i64,
}

fn view(row: SanctionRow) -> SanctionView {
	SanctionView {
		id: row.id,
		subject_user_id: row.sanction.subject_user_id,
		org_id: row.sanction.org_id,
		kind: row.sanction.kind.as_str().to_string(),
		reason_code: row.sanction.reason_code,
		reason_taxonomy_version: row.sanction.reason_taxonomy_version,
		scope_kind: row.sanction.scope.kind.as_str().to_string(),
		scope_id: row.sanction.scope.id,
		starts_at: row.sanction.starts_at,
		expires_at: row.sanction.expires_at,
		decided_by: row.sanction.decided_by,
		recorded_at: row.recorded_at,
	}
}

#[derive(Deserialize)]
struct RecordSanction {
	subject_user_id: String,
	org_id: Option<String>,
	kind: String,
	reason_code: String,
	#[serde(default = "default_taxonomy")]
	reason_taxonomy_version: u32,
	scope_kind: String,
	scope_id: String,
	starts_at: i64,
	expires_at: Option<i64>,
}

fn default_taxonomy() -> u32 {
	REASON_TAXONOMY_VERSION
}

async fn record_sanction(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<RecordSanction>,
) -> Response {
	if !user.allows("directory:manage") {
		return forbidden();
	}
	let Some(kind) = SanctionKind::parse(&request.kind) else {
		return (StatusCode::BAD_REQUEST, format!("unknown sanction kind `{}`", request.kind)).into_response();
	};
	let Some(scope_kind) = ScopeKind::parse(&request.scope_kind) else {
		return (
			StatusCode::BAD_REQUEST,
			format!("unknown scope kind `{}`", request.scope_kind),
		)
			.into_response();
	};
	let row = SanctionRow {
		id: new_id(),
		sanction: Sanction {
			subject_user_id: request.subject_user_id,
			org_id: request.org_id,
			kind,
			reason_code: request.reason_code,
			reason_taxonomy_version: request.reason_taxonomy_version,
			scope: ScopeRef {
				kind: scope_kind,
				id: request.scope_id,
			},
			starts_at: request.starts_at,
			expires_at: request.expires_at,
			decided_by: user.user_id.clone(),
		},
		decided_by: user.user_id.clone(),
		recorded_at: now(),
	};
	if let Err(message) = row.sanction.validate() {
		return (StatusCode::BAD_REQUEST, message).into_response();
	}
	if let Err(error) = state
		.metadata
		.record_sanction(&row.id, &row.sanction, &row.decided_by, row.recorded_at)
		.await
	{
		return storage_error(error);
	}
	(StatusCode::CREATED, Json(view(row))).into_response()
}

#[derive(Deserialize)]
struct ListQuery {
	user: Option<String>,
}

async fn list_sanctions(State(state): State<AppState>, user: AuthenticatedUser, Query(query): Query<ListQuery>) -> Response {
	if !user.allows("directory:manage") {
		return forbidden();
	}
	match state.metadata.sanctions_for(query.user.as_deref()).await {
		Ok(rows) => Json(rows.into_iter().map(view).collect::<Vec<_>>()).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn get_sanction(State(state): State<AppState>, Path(id): Path<String>, user: AuthenticatedUser) -> Response {
	if !user.allows("directory:manage") {
		return forbidden();
	}
	match state.metadata.sanction(&id).await {
		Ok(Some(row)) => Json(view(row)).into_response(),
		Ok(None) => (StatusCode::NOT_FOUND, "no such sanction").into_response(),
		Err(error) => storage_error(error),
	}
}

pub(crate) async fn publishing_block(state: &AppState, user_id: &str) -> Option<String> {
	let sanction = state
		.metadata
		.active_publishing_sanction(user_id, now())
		.await
		.ok()
		.flatten()?;
	Some(format!(
		"this account is under a {} ({})",
		sanction.sanction.kind.as_str(),
		sanction.sanction.reason_code
	))
}

fn forbidden() -> Response {
	(StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response()
}

fn new_id() -> String {
	let mut bytes = [0u8; 16];
	if getrandom::fill(&mut bytes).is_err() {
		panic!("operating system randomness is unavailable");
	}
	hex::encode(bytes)
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "sanction store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
