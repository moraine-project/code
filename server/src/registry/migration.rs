use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::delegation::Delegation;
use moraine_model::signed::SignedObject;
use serde::Serialize;
use sqlx::Row;

use crate::db::MetadataStore;
use crate::routes::AppState;

pub(crate) fn routes() -> Router<AppState> {
	Router::new().route("/v1/projects/{id}/migrations", get(list_migrations))
}

#[derive(Debug, Clone)]
pub struct MigrationRow {
	pub object_digest: Vec<u8>,
	pub old_home: String,
	pub new_home: String,
	pub cutover_seq: i64,
	pub reason: Option<String>,
	pub declared_time: i64,
}

impl MetadataStore {
	pub async fn record_migration(&self, project_id: &str, row: &MigrationRow, recorded_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO project_migrations (project_id, object_digest, old_home, new_home, cutover_seq, reason,
			 declared_time, recorded_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
			 ON CONFLICT(project_id, object_digest) DO NOTHING",
		)
		.bind(project_id)
		.bind(&row.object_digest)
		.bind(&row.old_home)
		.bind(&row.new_home)
		.bind(row.cutover_seq)
		.bind(row.reason.as_deref())
		.bind(row.declared_time)
		.bind(recorded_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn migrations_for(&self, project_id: &str) -> Result<Vec<MigrationRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT object_digest, old_home, new_home, cutover_seq, reason, declared_time FROM project_migrations
			 WHERE project_id = $1 ORDER BY declared_time DESC, cutover_seq DESC",
		)
		.bind(project_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| MigrationRow {
				object_digest: row.get("object_digest"),
				old_home: row.get("old_home"),
				new_home: row.get("new_home"),
				cutover_seq: row.get("cutover_seq"),
				reason: row.get("reason"),
				declared_time: row.get("declared_time"),
			})
			.collect())
	}
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
	let Delegation::Migration(migration) = &signed.payload else {
		return Ok(());
	};
	if migration.project_id != project_id {
		return Ok(());
	}
	let row = MigrationRow {
		object_digest: object_digest.to_vec(),
		old_home: migration.old_home.clone(),
		new_home: migration.new_home.clone(),
		cutover_seq: i64::try_from(migration.cutover_seq).unwrap_or(i64::MAX),
		reason: migration.reason.clone(),
		declared_time: migration.declared_time,
	};
	if let Err(error) = state.metadata.record_migration(project_id, &row, now()).await {
		return Err(Box::new(super::storage_error(error)));
	}
	Ok(())
}

#[derive(Serialize)]
struct MigrationView {
	migration: String,
	old_home: String,
	new_home: String,
	cutover_seq: i64,
	reason: Option<String>,
	declared_time: i64,
}

async fn list_migrations(State(state): State<AppState>, Path(id): Path<String>) -> Response {
	if let Err(response) = super::load_root(&state, &id).await {
		return *response;
	}
	match state.metadata.migrations_for(&id).await {
		Ok(rows) => Json(
			rows.into_iter()
				.map(|row| MigrationView {
					migration: super::id_for(&row.object_digest),
					old_home: row.old_home,
					new_home: row.new_home,
					cutover_seq: row.cutover_seq,
					reason: row.reason,
					declared_time: row.declared_time,
				})
				.collect::<Vec<_>>(),
		)
		.into_response(),
		Err(error) => super::storage_error(error),
	}
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}
