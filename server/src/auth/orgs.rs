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

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use axum::body::{Body, to_bytes};
	use axum::http::{Method, header};
	use tower::ServiceExt;

	use super::*;
	use crate::blob::BlobStore;
	use crate::capability::Capability;

	async fn app() -> (Router, tempfile::TempDir) {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = Arc::new(BlobStore::new(directory.path()).await.expect("blob store"));
		let metadata = Arc::new(
			MetadataStore::open(directory.path().join("metadata.sqlite"))
				.await
				.expect("metadata"),
		);
		let config = crate::config::Config {
			bind: "127.0.0.1:0".parse().expect("addr"),
			data_dir: directory.path().to_path_buf(),
			max_artifact_bytes: 1024,
			max_upload_bytes_per_account: 5_368_709_120,
			max_projects: 10_000,
			tls_terminated: false,
			allow_insecure_http: false,
			web_origins: Vec::new(),
			max_mirror_probes_per_cycle: 20,
			max_mirror_probe_bytes: 268_435_456,
			max_feed_page_entries: 100,
			max_response_bytes: 16_777_216,
			staging_retention_seconds: 3_600,
			blob_retention_seconds: 604_800,
			max_sync_pages: 200,
			requests_per_minute: 600,
			max_concurrent_syncs: 4,
			maintenance_interval_seconds: 3_600,
			tls_extra_roots: None,
			max_feed_scan_pages: 50,
			skip_migrate_on_start: false,
			database_url: None,
			allow_insecure_federation_local: false,
			publishing: crate::config::Publishing::Open,
			web_dir: None,
			s3: Default::default(),
		};
		let state = AppState {
			store,
			metadata,
			capability: Arc::new(Capability::discover(&config)),
			login_limiter: std::sync::Arc::new(crate::auth::LoginLimiter::new()),
			metrics: std::sync::Arc::new(crate::ops::metrics::Metrics::new()),
			rate_limiter: std::sync::Arc::new(crate::auth::ratelimit::RateLimiter::new()),
			web_dir: None,
		};
		(crate::routes::router(state), directory)
	}

	async fn login(app: &Router, email: &str) -> (String, String) {
		let credentials = serde_json::json!({ "email": email, "password": "correct horse battery" }).to_string();
		let register = axum::http::Request::post("/v1/auth/register")
			.header(header::CONTENT_TYPE, "application/json")
			.body(Body::from(credentials.clone()))
			.expect("request");
		let response = app.clone().oneshot(register).await.expect("response");
		let user_id = body_json(response).await["user_id"].as_str().expect("user id").to_string();
		let session = axum::http::Request::post("/v1/auth/session")
			.header(header::CONTENT_TYPE, "application/json")
			.body(Body::from(credentials))
			.expect("request");
		let response = app.clone().oneshot(session).await.expect("response");
		let session_token = set_cookie(&response, "moraine_session");
		let csrf = set_cookie(&response, "moraine_csrf");
		(user_id, format!("moraine_session={session_token}; moraine_csrf={csrf}"))
	}

	fn set_cookie(response: &Response, name: &str) -> String {
		response
			.headers()
			.get_all(header::SET_COOKIE)
			.iter()
			.find_map(|value| {
				let cookie = value.to_str().ok()?;
				cookie
					.split(';')
					.next()?
					.strip_prefix(&format!("{name}="))
					.map(str::to_string)
			})
			.unwrap_or_default()
	}

	fn csrf_of(cookie: &str) -> String {
		cookie
			.split(';')
			.find_map(|part| part.trim().strip_prefix("moraine_csrf="))
			.unwrap_or_default()
			.to_string()
	}

	async fn body_json(response: Response) -> serde_json::Value {
		let bytes = to_bytes(response.into_body(), 64 * 1024).await.expect("body");
		serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
	}

	fn write(path: &str, cookie: &str, body: serde_json::Value) -> axum::http::Request<Body> {
		axum::http::Request::builder()
			.method(Method::POST)
			.uri(path)
			.header(header::CONTENT_TYPE, "application/json")
			.header(header::COOKIE, cookie)
			.header("x-csrf-token", csrf_of(cookie))
			.body(Body::from(body.to_string()))
			.expect("request")
	}

	fn read(path: &str, cookie: &str) -> axum::http::Request<Body> {
		axum::http::Request::builder()
			.method(Method::GET)
			.uri(path)
			.header(header::COOKIE, cookie)
			.body(Body::empty())
			.expect("request")
	}

	#[tokio::test]
	async fn lists_the_orgs_an_account_belongs_to() {
		let (application, _directory) = app().await;
		let (_, owner_cookie) = login(&application, "owner@example.org").await;
		let (_, outsider_cookie) = login(&application, "outsider@example.org").await;
		let create = write(
			"/v1/orgs",
			&owner_cookie,
			serde_json::json!({ "handle": "cleanroom", "display_name": "Cleanroom" }),
		);
		let response = application.clone().oneshot(create).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let response = application
			.clone()
			.oneshot(read("/v1/orgs", &owner_cookie))
			.await
			.expect("response");
		let orgs = body_json(response).await;
		assert_eq!(orgs.as_array().expect("orgs").len(), 1);
		assert_eq!(orgs[0]["handle"], "cleanroom");
		assert_eq!(orgs[0]["display_name"], "Cleanroom");
		assert_eq!(orgs[0]["role"], "owner");

		let response = application
			.oneshot(read("/v1/orgs", &outsider_cookie))
			.await
			.expect("response");
		let orgs = body_json(response).await;
		assert!(orgs.as_array().expect("orgs").is_empty());
	}

	#[tokio::test]
	async fn owner_manages_members_and_last_owner_is_protected() {
		let (application, _directory) = app().await;
		let (owner_id, owner_cookie) = login(&application, "owner@example.org").await;
		let (member_id, member_cookie) = login(&application, "member@example.org").await;

		let create = write(
			"/v1/orgs",
			&owner_cookie,
			serde_json::json!({ "handle": "cleanroom", "display_name": "Cleanroom" }),
		);
		let response = application.clone().oneshot(create).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let add = write(
			"/v1/orgs/cleanroom/members",
			&owner_cookie,
			serde_json::json!({ "email": "member@example.org", "role": "member" }),
		);
		let response = application.clone().oneshot(add).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let members = application
			.clone()
			.oneshot(read("/v1/orgs/cleanroom/members", &member_cookie))
			.await
			.expect("response");
		let list = body_json(members).await;
		assert_eq!(list.as_array().expect("members").len(), 2);

		let member_adds = write(
			"/v1/orgs/cleanroom/members",
			&member_cookie,
			serde_json::json!({ "email": "owner@example.org", "role": "admin" }),
		);
		let response = application.clone().oneshot(member_adds).await.expect("response");
		assert_eq!(response.status(), StatusCode::FORBIDDEN);

		let remove_owner = axum::http::Request::builder()
			.method(Method::DELETE)
			.uri(format!("/v1/orgs/cleanroom/members/{owner_id}"))
			.header(header::COOKIE, &owner_cookie)
			.header("x-csrf-token", csrf_of(&owner_cookie))
			.body(Body::empty())
			.expect("request");
		let response = application.clone().oneshot(remove_owner).await.expect("response");
		assert_eq!(response.status(), StatusCode::CONFLICT);

		let member_removes_self = axum::http::Request::builder()
			.method(Method::DELETE)
			.uri(format!("/v1/orgs/cleanroom/members/{member_id}"))
			.header(header::COOKIE, &owner_cookie)
			.header("x-csrf-token", csrf_of(&owner_cookie))
			.body(Body::empty())
			.expect("request");
		let response = application.oneshot(member_removes_self).await.expect("response");
		assert_eq!(response.status(), StatusCode::NO_CONTENT);
	}

	#[tokio::test]
	async fn nested_teams_must_share_the_org() {
		let (application, _directory) = app().await;
		let (_owner_id, owner_cookie) = login(&application, "owner@example.org").await;
		application
			.clone()
			.oneshot(write(
				"/v1/orgs",
				&owner_cookie,
				serde_json::json!({ "handle": "team-org", "display_name": "Team Org" }),
			))
			.await
			.expect("response");
		let parent = body_json(
			application
				.clone()
				.oneshot(write(
					"/v1/orgs/team-org/teams",
					&owner_cookie,
					serde_json::json!({ "display_name": "Maintainers" }),
				))
				.await
				.expect("response"),
		)
		.await;
		let parent_id = parent["id"].as_str().expect("team id").to_string();

		let child = write(
			"/v1/orgs/team-org/teams",
			&owner_cookie,
			serde_json::json!({ "display_name": "Reviewers", "parent_team_id": parent_id }),
		);
		let response = application.clone().oneshot(child).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);
		let child_id = body_json(response).await["id"].as_str().expect("team id").to_string();

		let reparent = axum::http::Request::builder()
			.method(Method::PATCH)
			.uri(format!("/v1/orgs/team-org/teams/{parent_id}"))
			.header(header::CONTENT_TYPE, "application/json")
			.header(header::COOKIE, &owner_cookie)
			.header("x-csrf-token", csrf_of(&owner_cookie))
			.body(Body::from(serde_json::json!({ "parent_team_id": null }).to_string()))
			.expect("request");
		let response = application.clone().oneshot(reparent).await.expect("response");
		assert_eq!(response.status(), StatusCode::OK);
		let view = body_json(response).await;
		assert!(view["parent_team_id"].is_null());

		let cycle = axum::http::Request::builder()
			.method(Method::PATCH)
			.uri(format!("/v1/orgs/team-org/teams/{parent_id}"))
			.header(header::CONTENT_TYPE, "application/json")
			.header(header::COOKIE, &owner_cookie)
			.header("x-csrf-token", csrf_of(&owner_cookie))
			.body(Body::from(serde_json::json!({ "parent_team_id": child_id }).to_string()))
			.expect("request");
		let response = application.oneshot(cycle).await.expect("response");
		assert_eq!(response.status(), StatusCode::BAD_REQUEST);
	}
}
