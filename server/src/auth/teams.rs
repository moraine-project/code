use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::orgs::{new_id, now, require_role, storage_error};
use crate::auth::AuthenticatedUser;
use crate::db::MetadataStore;
use crate::routes::AppState;

#[derive(Debug, Clone)]
pub struct TeamRow {
	pub id: String,
	pub org_id: String,
	pub parent_team_id: Option<String>,
	pub display_name: String,
}

impl MetadataStore {
	pub async fn create_team(
		&self,
		id: &str,
		org_id: &str,
		parent_team_id: Option<&str>,
		display_name: &str,
		created_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT INTO teams (id, org_id, parent_team_id, display_name, created_at) VALUES ($1, $2, $3, $4, $5)")
			.bind(id)
			.bind(org_id)
			.bind(parent_team_id)
			.bind(display_name)
			.bind(created_at)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn org_teams(&self, org_id: &str) -> Result<Vec<TeamRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, org_id, parent_team_id, display_name FROM teams WHERE org_id = $1 ORDER BY created_at, id",
		)
		.bind(org_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| TeamRow {
				id: row.get("id"),
				org_id: row.get("org_id"),
				parent_team_id: row.get("parent_team_id"),
				display_name: row.get("display_name"),
			})
			.collect())
	}

	pub async fn team(&self, id: &str) -> Result<Option<TeamRow>, sqlx::Error> {
		let row = sqlx::query("SELECT id, org_id, parent_team_id, display_name FROM teams WHERE id = $1")
			.bind(id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.map(|row| TeamRow {
			id: row.get("id"),
			org_id: row.get("org_id"),
			parent_team_id: row.get("parent_team_id"),
			display_name: row.get("display_name"),
		}))
	}
}

#[derive(Serialize)]
pub(super) struct TeamView {
	pub(super) id: String,
	pub(super) parent_team_id: Option<String>,
	pub(super) display_name: String,
}

pub(super) async fn list_teams(
	State(state): State<AppState>,
	Path(handle): Path<String>,
	user: AuthenticatedUser,
) -> Response {
	let org = match require_role(&state, &handle, &user, &["owner", "admin", "member"]).await {
		Ok(org) => org,
		Err(response) => return *response,
	};
	match state.metadata.org_teams(&org.id).await {
		Ok(teams) => Json(
			teams
				.into_iter()
				.map(|team| TeamView {
					id: team.id,
					parent_team_id: team.parent_team_id,
					display_name: team.display_name,
				})
				.collect::<Vec<_>>(),
		)
		.into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
pub(super) struct CreateTeam {
	display_name: String,
	parent_team_id: Option<String>,
}

pub(super) async fn create_team(
	State(state): State<AppState>,
	Path(handle): Path<String>,
	user: AuthenticatedUser,
	Json(request): Json<CreateTeam>,
) -> Response {
	let org = match require_role(&state, &handle, &user, &["owner", "admin"]).await {
		Ok(org) => org,
		Err(response) => return *response,
	};
	if request.display_name.trim().is_empty() {
		return (StatusCode::BAD_REQUEST, "display_name is required").into_response();
	}
	if let Some(parent_id) = &request.parent_team_id {
		match state.metadata.team(parent_id).await {
			Ok(Some(parent)) if parent.org_id == org.id => {}
			Ok(_) => return (StatusCode::BAD_REQUEST, "parent team is not in this org").into_response(),
			Err(error) => return storage_error(error),
		}
	}
	let id = new_id();
	if let Err(error) = state
		.metadata
		.create_team(
			&id,
			&org.id,
			request.parent_team_id.as_deref(),
			request.display_name.trim(),
			now(),
		)
		.await
	{
		return storage_error(error);
	}
	let view = TeamView {
		id,
		parent_team_id: request.parent_team_id,
		display_name: request.display_name.trim().to_string(),
	};
	(StatusCode::CREATED, Json(view)).into_response()
}
