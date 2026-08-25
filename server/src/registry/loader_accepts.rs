use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::Canonical;
use moraine_model::definition::LoaderObject;
use moraine_model::genesis::GenesisKind;
use serde::Serialize;
use sqlx::Row;

use super::definitions::{id_for, load_definition};
use super::storage_error;
use crate::db::MetadataStore;
use crate::routes::AppState;

pub(crate) fn routes() -> Router<AppState> {
	Router::new().route("/v1/loaders/{id}/accepts", get(list_loader_accepts))
}

impl MetadataStore {
	pub async fn index_loader_acceptance(
		&self,
		acceptance: &moraine_model::definition::LoaderAcceptance,
		object_digest: &[u8],
	) -> Result<(), sqlx::Error> {
		let existing = sqlx::query(
			"SELECT object_digest, declared_time FROM loader_accepts WHERE accepting_loader_id = $1 AND accepted_loader_id = $2",
		)
		.bind(&acceptance.accepting_loader_id)
		.bind(&acceptance.accepted_loader_id)
		.fetch_optional(&self.pool)
		.await?;
		if let Some(row) = existing {
			let recorded: Vec<u8> = row.get("object_digest");
			if recorded == object_digest {
				return Ok(());
			}
			let recorded_at: i64 = row.get("declared_time");
			if acceptance.declared_time < recorded_at {
				return Err(sqlx::Error::Protocol(
					"a newer acceptance mapping for this pair is already published".to_string(),
				));
			}
		}
		sqlx::query(
			"INSERT INTO loader_accepts (accepting_loader_id, accepted_loader_id, object_digest, qualification,
			 declared_by_kind, declared_by_id, declared_time)
			 VALUES ($1, $2, $3, $4, $5, $6, $7)
			 ON CONFLICT(accepting_loader_id, accepted_loader_id) DO UPDATE SET object_digest = $3, qualification = $4,
			 declared_by_kind = $5, declared_by_id = $6, declared_time = $7",
		)
		.bind(&acceptance.accepting_loader_id)
		.bind(&acceptance.accepted_loader_id)
		.bind(object_digest)
		.bind(acceptance.qualification.as_str())
		.bind(&acceptance.declared_by.kind)
		.bind(&acceptance.declared_by.id)
		.bind(acceptance.declared_time)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn loader_accepts_for(&self, accepting_loader_id: &str) -> Result<Vec<Vec<u8>>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT object_digest FROM loader_accepts WHERE accepting_loader_id = $1 ORDER BY accepted_loader_id ASC",
		)
		.bind(accepting_loader_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(|row| row.get("object_digest")).collect())
	}
}

#[derive(Serialize)]
struct LoaderAcceptView {
	accepted_loader_id: String,
	accepted: String,
	qualification: String,
	declared_by: DeclaredByView,
	declared_time: i64,
	game_version_predicate: Option<serde_json::Value>,
	accepted_version_predicate: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct DeclaredByView {
	kind: String,
	id: String,
}

async fn list_loader_accepts(State(state): State<AppState>, Path(id): Path<String>) -> Response {
	if let Err(response) = load_definition(&state, GenesisKind::Loader, &id).await {
		return *response;
	}
	let rows = match state.metadata.loader_accepts_for(&id).await {
		Ok(rows) => rows,
		Err(error) => return storage_error(error),
	};
	let mut accepts = Vec::with_capacity(rows.len());
	for digest in rows {
		let Some(object) = (match state.metadata.object(&digest).await {
			Ok(object) => object,
			Err(error) => return storage_error(error),
		}) else {
			continue;
		};
		let Ok(LoaderObject::Acceptance(acceptance)) = LoaderObject::from_canonical_bytes(&object.payload) else {
			continue;
		};
		accepts.push(LoaderAcceptView {
			accepted_loader_id: acceptance.accepted_loader_id,
			accepted: id_for(&digest),
			qualification: acceptance.qualification.as_str().to_string(),
			declared_by: DeclaredByView {
				kind: acceptance.declared_by.kind,
				id: acceptance.declared_by.id,
			},
			declared_time: acceptance.declared_time,
			game_version_predicate: acceptance
				.game_version_predicate
				.as_ref()
				.map(crate::registry::views::predicate_json),
			accepted_version_predicate: acceptance
				.accepted_version_predicate
				.as_ref()
				.map(crate::registry::views::predicate_json),
		});
	}
	Json(accepts).into_response()
}

#[cfg(test)]
#[path = "loader_accepts_tests.rs"]
mod tests;
