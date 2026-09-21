use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::release::ReleaseObject;
use serde::Serialize;
use sqlx::Row;

use crate::auth::AuthenticatedUser;
use crate::db::MetadataStore;
use crate::routes::AppState;

#[derive(Debug, Clone)]
pub struct PublicationGrantRow {
	pub id: String,
	pub project_id: String,
	pub principal_kind: String,
	pub principal_id: String,
	pub game_id: String,
	pub release_kinds: String,
	pub artifact_kinds: String,
	pub issued_from_review: String,
	pub policy_version: String,
	pub issued_at: i64,
	pub expires_at: Option<i64>,
	pub suspended_at: Option<i64>,
	pub revoked_at: Option<i64>,
	pub reason_code: Option<String>,
}

impl MetadataStore {
	pub async fn active_publication_grant(
		&self,
		project_id: &str,
		principal_id: &str,
		game_id: &str,
		release_kind: &str,
		now: i64,
	) -> Result<Option<PublicationGrantRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT id, project_id, principal_kind, principal_id, game_id, release_kinds, artifact_kinds,
			 issued_from_review, policy_version, issued_at, expires_at, suspended_at, revoked_at, reason_code
			 FROM publication_grants
			 WHERE project_id = $1 AND principal_kind = 'user' AND principal_id = $2 AND game_id = $3
			 AND revoked_at IS NULL AND suspended_at IS NULL AND (expires_at IS NULL OR expires_at > $4)",
		)
		.bind(project_id)
		.bind(principal_id)
		.bind(game_id)
		.bind(now)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row
			.filter(|row| csv_contains(row.get("release_kinds"), release_kind))
			.map(grant_row))
	}

	pub async fn issue_publication_grant(&self, grant: &PublicationGrantRow) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO publication_grants (id, project_id, principal_kind, principal_id, game_id, release_kinds,
			 artifact_kinds, issued_from_review, policy_version, issued_at, expires_at, suspended_at, revoked_at, reason_code)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
		)
		.bind(&grant.id)
		.bind(&grant.project_id)
		.bind(&grant.principal_kind)
		.bind(&grant.principal_id)
		.bind(&grant.game_id)
		.bind(&grant.release_kinds)
		.bind(&grant.artifact_kinds)
		.bind(&grant.issued_from_review)
		.bind(&grant.policy_version)
		.bind(grant.issued_at)
		.bind(grant.expires_at)
		.bind(grant.suspended_at)
		.bind(grant.revoked_at)
		.bind(&grant.reason_code)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn publication_grants_for(&self, project_id: &str) -> Result<Vec<PublicationGrantRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, project_id, principal_kind, principal_id, game_id, release_kinds, artifact_kinds,
			 issued_from_review, policy_version, issued_at, expires_at, suspended_at, revoked_at, reason_code
			 FROM publication_grants WHERE project_id = $1 ORDER BY issued_at DESC, id ASC",
		)
		.bind(project_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(grant_row).collect())
	}

	pub async fn revoke_publication_grant(&self, id: &str, reason: &str, now: i64) -> Result<bool, sqlx::Error> {
		let result = sqlx::query(
			"UPDATE publication_grants SET revoked_at = $1, reason_code = $2
			 WHERE id = $3 AND revoked_at IS NULL",
		)
		.bind(now)
		.bind(reason)
		.bind(id)
		.execute(&self.pool)
		.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn suspend_publication_grants(&self, project_id: &str, reason: &str, now: i64) -> Result<(), sqlx::Error> {
		sqlx::query(
			"UPDATE publication_grants SET suspended_at = $1, reason_code = $2
			 WHERE project_id = $3 AND revoked_at IS NULL AND suspended_at IS NULL",
		)
		.bind(now)
		.bind(reason)
		.bind(project_id)
		.execute(&self.pool)
		.await?;
		Ok(())
	}
}

pub fn routes() -> Router<AppState> {
	Router::new().route("/v1/projects/{id}/publication-grants", get(list).delete(revoke))
}

async fn list(State(state): State<AppState>, user: AuthenticatedUser, Path(project_id): Path<String>) -> Response {
	if !user.allows("submissions:review") {
		return forbidden();
	}
	match state.metadata.publication_grants_for(&project_id).await {
		Ok(grants) => Json(grants.into_iter().map(view).collect::<Vec<_>>()).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn revoke(State(state): State<AppState>, user: AuthenticatedUser, Path(project_id): Path<String>) -> Response {
	if !user.allows("submissions:review") {
		return forbidden();
	}
	let grants = match state.metadata.publication_grants_for(&project_id).await {
		Ok(grants) => grants,
		Err(error) => return storage_error(error),
	};
	let mut revoked = 0;
	for grant in grants {
		if grant.revoked_at.is_none()
			&& matches!(
				state
					.metadata
					.revoke_publication_grant(&grant.id, "operator-revoked", now())
					.await,
				Ok(true)
			) {
			revoked += 1;
		}
	}
	Json(serde_json::json!({ "revoked": revoked })).into_response()
}

#[derive(Serialize)]
struct GrantView {
	id: String,
	project_id: String,
	principal: String,
	game_id: String,
	release_kinds: String,
	artifact_kinds: String,
	issued_from_review: String,
	policy_version: String,
	issued_at: i64,
	expires_at: Option<i64>,
	suspended_at: Option<i64>,
	revoked_at: Option<i64>,
	reason_code: Option<String>,
}

fn view(grant: PublicationGrantRow) -> GrantView {
	GrantView {
		id: grant.id,
		project_id: grant.project_id,
		principal: format!("{}:{}", grant.principal_kind, grant.principal_id),
		game_id: grant.game_id,
		release_kinds: grant.release_kinds,
		artifact_kinds: grant.artifact_kinds,
		issued_from_review: grant.issued_from_review,
		policy_version: grant.policy_version,
		issued_at: grant.issued_at,
		expires_at: grant.expires_at,
		suspended_at: grant.suspended_at,
		revoked_at: grant.revoked_at,
		reason_code: grant.reason_code,
	}
}

fn grant_row(row: sqlx::any::AnyRow) -> PublicationGrantRow {
	PublicationGrantRow {
		id: row.get("id"),
		project_id: row.get("project_id"),
		principal_kind: row.get("principal_kind"),
		principal_id: row.get("principal_id"),
		game_id: row.get("game_id"),
		release_kinds: row.get("release_kinds"),
		artifact_kinds: row.get("artifact_kinds"),
		issued_from_review: row.get("issued_from_review"),
		policy_version: row.get("policy_version"),
		issued_at: row.get("issued_at"),
		expires_at: row.get("expires_at"),
		suspended_at: row.get("suspended_at"),
		revoked_at: row.get("revoked_at"),
		reason_code: row.get("reason_code"),
	}
}

fn csv_contains(value: String, expected: &str) -> bool {
	value.split(',').any(|item| item == expected)
}

fn forbidden() -> Response {
	(StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response()
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "publication grant store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

pub(crate) async fn release_scope(state: &crate::routes::AppState, object_digest: &[u8]) -> Option<(String, String)> {
	let object = state.metadata.object(object_digest).await.ok()??;
	let signed = moraine_model::signed::SignedObject::<ReleaseObject>::from_bytes(&object.wire).ok()?;
	match signed.payload {
		ReleaseObject::Release(release) => Some((release.game_id, release.kind)),
		ReleaseObject::Location(_) | ReleaseObject::Withdrawal(_) => None,
	}
}

pub(crate) fn new_id() -> String {
	let mut bytes = [0u8; 16];
	if getrandom::fill(&mut bytes).is_err() {
		panic!("operating system randomness is unavailable");
	}
	hex::encode(bytes)
}
