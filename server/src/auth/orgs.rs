use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::moderation::valid_handle;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::teams::TeamView;
use crate::auth::AuthenticatedUser;
use crate::db::MetadataStore;
use crate::routes::AppState;

const ROLES: &[&str] = &["owner", "admin", "member"];

#[derive(Debug, Clone)]
pub struct OrgRow {
	pub id: String,
	pub handle: String,
	pub display_name: String,
	pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrgMembership {
	pub id: String,
	pub handle: String,
	pub display_name: String,
	pub role: String,
}

#[derive(Debug, Clone)]
pub struct OrgMemberRow {
	pub user_id: String,
	pub email: String,
	pub role: String,
	pub added_at: i64,
}

impl MetadataStore {
	pub async fn create_org(&self, id: &str, handle: &str, display_name: &str, created_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT INTO orgs (id, handle, display_name, created_at) VALUES ($1, $2, $3, $4)")
			.bind(id)
			.bind(handle)
			.bind(display_name)
			.bind(created_at)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn org_by_handle(&self, handle: &str) -> Result<Option<OrgRow>, sqlx::Error> {
		let row = sqlx::query("SELECT id, handle, display_name, created_at FROM orgs WHERE handle = $1")
			.bind(handle)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.map(|row| OrgRow {
			id: row.get("id"),
			handle: row.get("handle"),
			display_name: row.get("display_name"),
			created_at: row.get("created_at"),
		}))
	}

	pub async fn add_org_member(&self, org_id: &str, user_id: &str, role: &str, added_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO org_members (org_id, user_id, role, added_at) VALUES ($1, $2, $3, $4)
			 ON CONFLICT(org_id, user_id) DO UPDATE SET role = $3",
		)
		.bind(org_id)
		.bind(user_id)
		.bind(role)
		.bind(added_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn delete_org_member(&self, org_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
		let result = sqlx::query("DELETE FROM org_members WHERE org_id = $1 AND user_id = $2")
			.bind(org_id)
			.bind(user_id)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn orgs_for_user(&self, user_id: &str) -> Result<Vec<OrgMembership>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT o.id, o.handle, o.display_name, m.role FROM org_members m JOIN orgs o ON o.id = m.org_id
			 WHERE m.user_id = $1 ORDER BY m.added_at, o.handle",
		)
		.bind(user_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| OrgMembership {
				id: row.get("id"),
				handle: row.get("handle"),
				display_name: row.get("display_name"),
				role: row.get("role"),
			})
			.collect())
	}

	pub async fn org_role(&self, org_id: &str, user_id: &str) -> Result<Option<String>, sqlx::Error> {
		let row = sqlx::query("SELECT role FROM org_members WHERE org_id = $1 AND user_id = $2")
			.bind(org_id)
			.bind(user_id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.map(|row| row.get("role")))
	}

	pub async fn org_members(&self, org_id: &str) -> Result<Vec<OrgMemberRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT m.user_id, u.email, m.role, m.added_at FROM org_members m JOIN users u ON u.id = m.user_id
			 WHERE m.org_id = $1 ORDER BY m.added_at, m.user_id",
		)
		.bind(org_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| OrgMemberRow {
				user_id: row.get("user_id"),
				email: row.get("email"),
				role: row.get("role"),
				added_at: row.get("added_at"),
			})
			.collect())
	}

	pub async fn org_owner_count(&self, org_id: &str) -> Result<i64, sqlx::Error> {
		let row = sqlx::query("SELECT COUNT(*) AS owners FROM org_members WHERE org_id = $1 AND role = 'owner'")
			.bind(org_id)
			.fetch_one(&self.pool)
			.await?;
		Ok(row.get("owners"))
	}
}

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/orgs", get(list_orgs).post(create_org))
		.route("/v1/orgs/{handle}", get(org_detail))
		.route("/v1/orgs/{handle}/members", get(list_members).post(add_member))
		.route("/v1/orgs/{handle}/members/{user_id}", axum::routing::delete(remove_member))
		.route(
			"/v1/orgs/{handle}/teams",
			get(super::teams::list_teams).post(super::teams::create_team),
		)
		.route(
			"/v1/orgs/{handle}/teams/{team_id}",
			axum::routing::patch(super::teams::reparent_team),
		)
}

#[derive(Deserialize)]
struct CreateOrg {
	handle: String,
	display_name: String,
}

#[derive(Serialize)]
struct OrgReceipt {
	id: String,
	handle: String,
}

#[derive(Serialize)]
struct OrgView {
	id: String,
	handle: String,
	display_name: String,
	created_at: i64,
	teams: Vec<TeamView>,
	projects: Vec<String>,
}

#[derive(Serialize)]
struct MemberView {
	user_id: String,
	email: String,
	role: String,
	added_at: i64,
}

async fn list_orgs(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	match state.metadata.orgs_for_user(&user.user_id).await {
		Ok(orgs) => Json(orgs).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn create_org(State(state): State<AppState>, user: AuthenticatedUser, Json(request): Json<CreateOrg>) -> Response {
	let handle = request.handle.trim().to_lowercase();
	if !valid_handle(&handle) {
		return (StatusCode::BAD_REQUEST, "invalid handle").into_response();
	}
	if request.display_name.trim().is_empty() {
		return (StatusCode::BAD_REQUEST, "display_name is required").into_response();
	}
	match state.metadata.org_by_handle(&handle).await {
		Ok(Some(_)) => return (StatusCode::CONFLICT, "handle is taken").into_response(),
		Ok(None) => {}
		Err(error) => return storage_error(error),
	}
	let id = new_id();
	if let Err(error) = state
		.metadata
		.create_org(&id, &handle, request.display_name.trim(), now())
		.await
	{
		return storage_error(error);
	}
	if let Err(error) = state.metadata.add_org_member(&id, &user.user_id, "owner", now()).await {
		return storage_error(error);
	}
	(StatusCode::CREATED, Json(OrgReceipt { id, handle })).into_response()
}

async fn org_detail(State(state): State<AppState>, Path(handle): Path<String>, user: AuthenticatedUser) -> Response {
	let org = match load_org(&state, &handle).await {
		Ok(org) => org,
		Err(response) => return *response,
	};
	let role = match state.metadata.org_role(&org.id, &user.user_id).await {
		Ok(role) => role,
		Err(error) => return storage_error(error),
	};
	if role.is_none() {
		return (StatusCode::FORBIDDEN, "not an org member").into_response();
	}
	let teams = match state.metadata.org_teams(&org.id).await {
		Ok(teams) => teams,
		Err(error) => return storage_error(error),
	};
	let projects = match state.metadata.projects_owned_by("org", &org.id).await {
		Ok(projects) => projects,
		Err(error) => return storage_error(error),
	};
	let view = OrgView {
		id: org.id,
		handle: org.handle,
		display_name: org.display_name,
		created_at: org.created_at,
		teams: teams
			.into_iter()
			.map(|team| TeamView {
				id: team.id,
				parent_team_id: team.parent_team_id,
				display_name: team.display_name,
			})
			.collect(),
		projects,
	};
	Json(view).into_response()
}

async fn list_members(State(state): State<AppState>, Path(handle): Path<String>, user: AuthenticatedUser) -> Response {
	let org = match require_role(&state, &handle, &user, &["owner", "admin", "member"]).await {
		Ok(org) => org,
		Err(response) => return *response,
	};
	match state.metadata.org_members(&org.id).await {
		Ok(members) => Json(members.into_iter().map(member_view).collect::<Vec<_>>()).into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
struct AddMember {
	email: String,
	role: String,
}

async fn add_member(
	State(state): State<AppState>,
	Path(handle): Path<String>,
	user: AuthenticatedUser,
	Json(request): Json<AddMember>,
) -> Response {
	let org = match require_role(&state, &handle, &user, &["owner", "admin"]).await {
		Ok(org) => org,
		Err(response) => return *response,
	};
	if !ROLES.contains(&request.role.as_str()) {
		return (StatusCode::BAD_REQUEST, "unknown role").into_response();
	}
	let actor_role = state
		.metadata
		.org_role(&org.id, &user.user_id)
		.await
		.ok()
		.flatten()
		.unwrap_or_default();
	if request.role == "owner" && actor_role != "owner" {
		return (StatusCode::FORBIDDEN, "only an owner may grant owner").into_response();
	}
	let email = request.email.trim().to_lowercase();
	let target = match state.metadata.user_by_email(&email).await {
		Ok(Some(record)) => record,
		Ok(None) => return (StatusCode::NOT_FOUND, "no account with that email").into_response(),
		Err(error) => return storage_error(error),
	};
	let current_role = state.metadata.org_role(&org.id, &target.id).await.ok().flatten();
	if current_role.as_deref() == Some("owner") && request.role != "owner" {
		if actor_role != "owner" {
			return (StatusCode::FORBIDDEN, "only an owner may change an owner's role").into_response();
		}
		let owners = state
			.metadata
			.org_members(&org.id)
			.await
			.map(|members| members.iter().filter(|member| member.role == "owner").count())
			.unwrap_or(0);
		if owners <= 1 {
			return (StatusCode::BAD_REQUEST, "an organization must keep at least one owner").into_response();
		}
	}
	if let Err(error) = state.metadata.add_org_member(&org.id, &target.id, &request.role, now()).await {
		return storage_error(error);
	}
	(
		StatusCode::CREATED,
		Json(OrgReceipt {
			id: target.id,
			handle: org.handle,
		}),
	)
		.into_response()
}

async fn remove_member(
	State(state): State<AppState>,
	Path((handle, user_id)): Path<(String, String)>,
	user: AuthenticatedUser,
) -> Response {
	let org = match require_role(&state, &handle, &user, &["owner", "admin"]).await {
		Ok(org) => org,
		Err(response) => return *response,
	};
	match state.metadata.org_role(&org.id, &user_id).await {
		Ok(Some(role)) if role == "owner" => match state.metadata.org_owner_count(&org.id).await {
			Ok(count) if count <= 1 => return (StatusCode::CONFLICT, "an org must keep at least one owner").into_response(),
			Ok(_) => {}
			Err(error) => return storage_error(error),
		},
		Ok(_) => {}
		Err(error) => return storage_error(error),
	}
	match state.metadata.delete_org_member(&org.id, &user_id).await {
		Ok(true) => StatusCode::NO_CONTENT.into_response(),
		Ok(false) => (StatusCode::NOT_FOUND, "no such member").into_response(),
		Err(error) => storage_error(error),
	}
}

async fn load_org(state: &AppState, handle: &str) -> Result<OrgRow, Box<Response>> {
	match state.metadata.org_by_handle(handle).await {
		Ok(Some(org)) => Ok(org),
		Ok(None) => Err(Box::new((StatusCode::NOT_FOUND, "no such org").into_response())),
		Err(error) => Err(Box::new(storage_error(error))),
	}
}

pub(super) async fn require_role(
	state: &AppState,
	handle: &str,
	user: &AuthenticatedUser,
	allowed: &[&str],
) -> Result<OrgRow, Box<Response>> {
	let org = load_org(state, handle).await?;
	let role = match state.metadata.org_role(&org.id, &user.user_id).await {
		Ok(Some(role)) => role,
		Ok(None) => return Err(Box::new((StatusCode::FORBIDDEN, "not an org member").into_response())),
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	if !allowed.contains(&role.as_str()) {
		return Err(Box::new(
			(StatusCode::FORBIDDEN, "role does not permit this action").into_response(),
		));
	}
	Ok(org)
}

fn member_view(member: OrgMemberRow) -> MemberView {
	MemberView {
		user_id: member.user_id,
		email: member.email,
		role: member.role,
		added_at: member.added_at,
	}
}

pub(super) fn new_id() -> String {
	let mut bytes = [0u8; 16];
	if getrandom::fill(&mut bytes).is_err() {
		panic!("operating system randomness is unavailable");
	}
	hex::encode(bytes)
}

pub(super) fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

pub(super) fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "org store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
