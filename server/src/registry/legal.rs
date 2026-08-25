use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::moderation::{LegalAction, LegalRequest, LegalRequestKind, LegalTarget};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::auth::AuthenticatedUser;
use crate::db::MetadataStore;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/legal-requests", get(list_requests).post(record_request))
		.route("/v1/legal-requests/{id}", get(get_request))
}

#[derive(Debug, Clone)]
pub struct LegalRequestRow {
	pub id: String,
	pub request: LegalRequest,
	pub recorded_by: String,
	pub recorded_at: i64,
}

impl MetadataStore {
	pub async fn record_legal_request(
		&self,
		id: &str,
		request: &LegalRequest,
		recorded_by: &str,
		recorded_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO legal_requests (id, kind, claimant_ref, target_kind, target_id, stated_basis, received_at,
			 action_taken, designated_agent_ref, responds_to, recorded_by, recorded_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
		)
		.bind(id)
		.bind(request.kind.as_str())
		.bind(&request.claimant_ref)
		.bind(request.target_kind.as_str())
		.bind(&request.target_id)
		.bind(&request.stated_basis)
		.bind(request.received_at)
		.bind(request.action_taken.as_str())
		.bind(request.designated_agent_ref.as_deref())
		.bind(request.responds_to.as_deref())
		.bind(recorded_by)
		.bind(recorded_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn legal_request(&self, id: &str) -> Result<Option<LegalRequestRow>, sqlx::Error> {
		let row = sqlx::query(sqlx::AssertSqlSafe(format!("{LEGAL_SELECT} WHERE id = $1")))
			.bind(id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.and_then(legal_row))
	}

	pub async fn legal_requests_for(&self, target_kind: &str, target_id: &str) -> Result<Vec<LegalRequestRow>, sqlx::Error> {
		let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
			"{LEGAL_SELECT} WHERE target_kind = $1 AND target_id = $2 ORDER BY received_at DESC, id ASC"
		)))
		.bind(target_kind)
		.bind(target_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().filter_map(legal_row).collect())
	}
}

const LEGAL_SELECT: &str = "SELECT id, kind, claimant_ref, target_kind, target_id, stated_basis, received_at, action_taken, designated_agent_ref, responds_to, recorded_by, recorded_at FROM legal_requests";

fn legal_row(row: sqlx::any::AnyRow) -> Option<LegalRequestRow> {
	let kind: String = row.get("kind");
	let target_kind: String = row.get("target_kind");
	let action_taken: String = row.get("action_taken");
	Some(LegalRequestRow {
		id: row.get("id"),
		request: LegalRequest {
			kind: LegalRequestKind::parse(&kind)?,
			claimant_ref: row.get("claimant_ref"),
			target_kind: LegalTarget::parse(&target_kind)?,
			target_id: row.get("target_id"),
			stated_basis: row.get("stated_basis"),
			received_at: row.get("received_at"),
			action_taken: LegalAction::parse(&action_taken)?,
			designated_agent_ref: row.get("designated_agent_ref"),
			responds_to: row.get("responds_to"),
		},
		recorded_by: row.get("recorded_by"),
		recorded_at: row.get("recorded_at"),
	})
}

#[derive(Serialize)]
struct LegalRequestView {
	id: String,
	kind: String,
	claimant_ref: String,
	target_kind: String,
	target_id: String,
	stated_basis: String,
	received_at: i64,
	action_taken: String,
	designated_agent_ref: Option<String>,
	responds_to: Option<String>,
	recorded_by: String,
	recorded_at: i64,
}

fn view(row: LegalRequestRow) -> LegalRequestView {
	LegalRequestView {
		id: row.id,
		kind: row.request.kind.as_str().to_string(),
		claimant_ref: row.request.claimant_ref,
		target_kind: row.request.target_kind.as_str().to_string(),
		target_id: row.request.target_id,
		stated_basis: row.request.stated_basis,
		received_at: row.request.received_at,
		action_taken: row.request.action_taken.as_str().to_string(),
		designated_agent_ref: row.request.designated_agent_ref,
		responds_to: row.request.responds_to,
		recorded_by: row.recorded_by,
		recorded_at: row.recorded_at,
	}
}

#[derive(Deserialize)]
struct RecordRequest {
	kind: String,
	claimant_ref: String,
	target_kind: String,
	target_id: String,
	stated_basis: String,
	received_at: i64,
	#[serde(default = "default_action")]
	action_taken: String,
	designated_agent_ref: Option<String>,
	responds_to: Option<String>,
}

fn default_action() -> String {
	LegalAction::None.as_str().to_string()
}

async fn record_request(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<RecordRequest>,
) -> Response {
	if !user.allows("directory:manage") {
		return forbidden();
	}
	let Some(kind) = LegalRequestKind::parse(&request.kind) else {
		return (StatusCode::BAD_REQUEST, format!("unknown request kind `{}`", request.kind)).into_response();
	};
	let Some(target_kind) = LegalTarget::parse(&request.target_kind) else {
		return (
			StatusCode::BAD_REQUEST,
			format!("unknown target kind `{}`", request.target_kind),
		)
			.into_response();
	};
	let Some(action_taken) = LegalAction::parse(&request.action_taken) else {
		return (StatusCode::BAD_REQUEST, format!("unknown action `{}`", request.action_taken)).into_response();
	};
	if let Some(prior) = &request.responds_to
		&& let Ok(None) = state.metadata.legal_request(prior).await
	{
		return (StatusCode::BAD_REQUEST, "responds_to names no recorded request").into_response();
	}
	let entry = LegalRequestRow {
		id: new_id(),
		request: LegalRequest {
			kind,
			claimant_ref: request.claimant_ref,
			target_kind,
			target_id: request.target_id,
			stated_basis: request.stated_basis,
			received_at: request.received_at,
			action_taken,
			designated_agent_ref: request.designated_agent_ref,
			responds_to: request.responds_to,
		},
		recorded_by: user.user_id.clone(),
		recorded_at: now(),
	};
	if let Err(message) = entry.request.validate() {
		return (StatusCode::BAD_REQUEST, message).into_response();
	}
	if let Err(error) = state
		.metadata
		.record_legal_request(&entry.id, &entry.request, &entry.recorded_by, entry.recorded_at)
		.await
	{
		return storage_error(error);
	}
	let id = entry.id.clone();
	(
		StatusCode::CREATED,
		Json(serde_json::json!({ "id": id, "request": view(entry) })),
	)
		.into_response()
}

#[derive(Deserialize)]
struct ListQuery {
	target_kind: String,
	target_id: String,
}

async fn list_requests(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	axum::extract::Query(query): axum::extract::Query<ListQuery>,
) -> Response {
	if !user.allows("directory:manage") {
		return forbidden();
	}
	let Some(target_kind) = LegalTarget::parse(&query.target_kind) else {
		return (
			StatusCode::BAD_REQUEST,
			format!("unknown target kind `{}`", query.target_kind),
		)
			.into_response();
	};
	match state
		.metadata
		.legal_requests_for(target_kind.as_str(), &query.target_id)
		.await
	{
		Ok(rows) => Json(rows.into_iter().map(view).collect::<Vec<_>>()).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn get_request(State(state): State<AppState>, Path(id): Path<String>, user: AuthenticatedUser) -> Response {
	if !user.allows("directory:manage") {
		return forbidden();
	}
	match state.metadata.legal_request(&id).await {
		Ok(Some(row)) => Json(view(row)).into_response(),
		Ok(None) => (StatusCode::NOT_FOUND, "no such request").into_response(),
		Err(error) => storage_error(error),
	}
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
	tracing::error!(%error, "legal request store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
