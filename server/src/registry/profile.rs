use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::Canonical;
use moraine_model::profile::ProfileRevision;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::{id_for, storage_error};
use crate::db::MetadataStore;
use crate::routes::AppState;

pub(crate) fn routes() -> Router<AppState> {
	Router::new().route("/v1/projects/{id}/profile", get(profile_view))
}

#[derive(Deserialize)]
struct ProfileQuery {
	at: Option<i64>,
}

impl MetadataStore {
	pub async fn profile_revision_at(&self, project_id: &str, at: i64) -> Result<Option<Vec<u8>>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT object_digest FROM feed_entries WHERE project_id = $1 AND kind = 'profile-updated' AND seq <= $2
			 ORDER BY seq DESC LIMIT 1",
		)
		.bind(project_id)
		.bind(at)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| row.get("object_digest")))
	}
}

#[derive(Serialize)]
struct ProfileView {
	project_id: String,
	display_name: String,
	summary: String,
	description: String,
	categories: Vec<String>,
	tags: Vec<String>,
	links: Vec<LinkView>,
	communities: Vec<LinkView>,
	revision: String,
}

#[derive(Serialize)]
struct LinkView {
	kind: String,
	url: String,
}

async fn profile_view(State(state): State<AppState>, Path(id): Path<String>, Query(query): Query<ProfileQuery>) -> Response {
	let project = match state.metadata.project(&id).await {
		Ok(Some(project)) => project,
		Ok(None) => return (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => return storage_error(error),
	};
	let revision_digest = match query.at {
		None => project.profile_digest,
		Some(seq) => {
			let revision = match state.metadata.profile_revision_at(&id, seq).await {
				Ok(revision) => revision,
				Err(error) => return storage_error(error),
			};
			let Some(revision) = revision else {
				return (StatusCode::NOT_FOUND, "no profile revision at or before that sequence").into_response();
			};
			Some(revision)
		}
	};
	let Some(revision_digest) = revision_digest else {
		return (StatusCode::NOT_FOUND, "no profile published").into_response();
	};
	let Some(object) = (match state.metadata.object(&revision_digest).await {
		Ok(object) => object,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "profile object is missing").into_response();
	};
	let Ok(profile) = ProfileRevision::from_canonical_bytes(&object.payload) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "stored profile does not decode").into_response();
	};
	let view = ProfileView {
		project_id: profile.project_id,
		display_name: profile.display_name,
		summary: profile.summary,
		description: profile.description,
		categories: profile.categories,
		tags: profile.tags,
		links: profile.links.into_iter().map(link_view).collect(),
		communities: profile.communities.into_iter().map(link_view).collect(),
		revision: id_for(&revision_digest),
	};
	Json(view).into_response()
}

fn link_view(link: moraine_model::profile::Link) -> LinkView {
	LinkView {
		kind: link.kind,
		url: link.url,
	}
}
