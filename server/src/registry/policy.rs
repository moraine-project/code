use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::moderation::ReasonCode;
use moraine_model::search::ListingState;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::auth::AuthenticatedUser;
use crate::db::MetadataStore;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
	Router::new().route("/v1/directory/policy/{project_id}", get(get_policy).put(put_policy))
}

#[derive(Debug, Clone)]
pub struct PolicyRow {
	pub listing_state: ListingState,
	pub reason_code: Option<String>,
	pub reason_note: Option<String>,
	pub updated_at: i64,
}

impl MetadataStore {
	pub async fn set_listing_policy(
		&self,
		project_id: &str,
		listing_state: ListingState,
		reason_code: Option<&str>,
		reason_note: Option<&str>,
		updated_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO directory_policy (project_id, listing_state, reason_code, reason_note, updated_at)
			 VALUES ($1, $2, $3, $4, $5)
			 ON CONFLICT(project_id) DO UPDATE SET listing_state = $2, reason_code = $3, reason_note = $4, updated_at = $5",
		)
		.bind(project_id)
		.bind(listing_state.as_str())
		.bind(reason_code)
		.bind(reason_note)
		.bind(updated_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn clear_listing_policy(&self, project_id: &str) -> Result<(), sqlx::Error> {
		sqlx::query("DELETE FROM directory_policy WHERE project_id = $1")
			.bind(project_id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn listing_policy(&self, project_id: &str) -> Result<Option<PolicyRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT listing_state, reason_code, reason_note, updated_at FROM directory_policy WHERE project_id = $1",
		)
		.bind(project_id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.and_then(policy_from_row))
	}

	pub async fn listing_policies(&self, project_ids: &[String]) -> Result<HashMap<String, PolicyRow>, sqlx::Error> {
		let mut policies = HashMap::new();
		if project_ids.is_empty() {
			return Ok(policies);
		}
		let mut builder = crate::db::sql::SqlBuilder::new(
			"SELECT project_id, listing_state, reason_code, reason_note, updated_at FROM directory_policy WHERE project_id IN (",
		);
		{
			let mut separated = builder.separated(", ");
			for project_id in project_ids {
				separated.push_bind(project_id);
			}
		}
		builder.push(")");
		for row in builder.into_query().fetch_all(&self.pool).await? {
			let project_id: String = row.get("project_id");
			if let Some(policy) = policy_from_row(row) {
				policies.insert(project_id, policy);
			}
		}
		Ok(policies)
	}
}

fn policy_from_row(row: sqlx::any::AnyRow) -> Option<PolicyRow> {
	let state: String = row.get("listing_state");
	Some(PolicyRow {
		listing_state: ListingState::parse(&state)?,
		reason_code: row.get("reason_code"),
		reason_note: row.get("reason_note"),
		updated_at: row.get("updated_at"),
	})
}

#[derive(Serialize)]
struct PolicyView {
	project_id: String,
	listing_state: ListingState,
	reason_code: Option<String>,
	reason_note: Option<String>,
	updated_at: Option<i64>,
}

async fn get_policy(State(state): State<AppState>, Path(project_id): Path<String>) -> Response {
	let policy = match state.metadata.listing_policy(&project_id).await {
		Ok(policy) => policy,
		Err(error) => return storage_error(error),
	};
	Json(PolicyView {
		project_id,
		listing_state: policy.as_ref().map(|row| row.listing_state).unwrap_or(ListingState::Listed),
		reason_code: policy.as_ref().and_then(|row| row.reason_code.clone()),
		reason_note: policy.as_ref().and_then(|row| row.reason_note.clone()),
		updated_at: policy.map(|row| row.updated_at),
	})
	.into_response()
}

#[derive(Deserialize)]
struct PolicyRequest {
	listing_state: ListingState,
	reason_code: Option<String>,
	reason_note: Option<String>,
}

async fn put_policy(
	State(state): State<AppState>,
	Path(project_id): Path<String>,
	user: AuthenticatedUser,
	Json(request): Json<PolicyRequest>,
) -> Response {
	if !user.allows("directory:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	if matches!(request.listing_state, ListingState::Listed) {
		if let Err(error) = state.metadata.clear_listing_policy(&project_id).await {
			return storage_error(error);
		}
		return Json(PolicyView {
			project_id,
			listing_state: ListingState::Listed,
			reason_code: None,
			reason_note: None,
			updated_at: None,
		})
		.into_response();
	}
	if let Some(code) = &request.reason_code
		&& ReasonCode::parse(code).is_none()
	{
		return (StatusCode::BAD_REQUEST, format!("unknown reason code `{code}`")).into_response();
	}
	let updated_at = now();
	if let Err(error) = state
		.metadata
		.set_listing_policy(
			&project_id,
			request.listing_state,
			request.reason_code.as_deref(),
			request.reason_note.as_deref(),
			updated_at,
		)
		.await
	{
		return storage_error(error);
	}
	Json(PolicyView {
		project_id,
		listing_state: request.listing_state,
		reason_code: request.reason_code,
		reason_note: request.reason_note,
		updated_at: Some(updated_at),
	})
	.into_response()
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "directory policy store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
