use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::Canonical;
use moraine_model::release::ReleaseObject;
use serde::Serialize;
use sqlx::Row;

use crate::db::MetadataStore;
use crate::routes::AppState;

pub(crate) fn routes() -> Router<AppState> {
	Router::new().route("/v1/projects/{id}/channels", get(list_channels))
}

impl MetadataStore {
	pub async fn release_published_entries(&self, project_id: &str) -> Result<Vec<(i64, Vec<u8>)>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT seq, object_digest FROM feed_entries WHERE project_id = $1 AND kind = 'release-published'
			 ORDER BY seq ASC",
		)
		.bind(project_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| (row.get("seq"), row.get("object_digest")))
			.collect())
	}
}

#[derive(Serialize)]
struct ChannelView {
	channel: String,
	release: String,
	human_version: String,
	seq: i64,
}

async fn list_channels(State(state): State<AppState>, Path(id): Path<String>) -> Response {
	match state.metadata.project(&id).await {
		Ok(Some(_)) => {}
		Ok(None) => return (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => return super::storage_error(error),
	}
	let entries = match state.metadata.release_published_entries(&id).await {
		Ok(entries) => entries,
		Err(error) => return super::storage_error(error),
	};
	let mut channels: std::collections::BTreeMap<String, ChannelView> = std::collections::BTreeMap::new();
	for (seq, digest) in entries {
		let Some(object) = (match state.metadata.object(&digest).await {
			Ok(object) => object,
			Err(error) => return super::storage_error(error),
		}) else {
			continue;
		};
		let Ok(ReleaseObject::Release(release)) = ReleaseObject::from_canonical_bytes(&object.payload) else {
			continue;
		};
		channels.insert(
			release.channel.clone(),
			ChannelView {
				channel: release.channel,
				release: super::id_for(&digest),
				human_version: release.human_version,
				seq,
			},
		);
	}
	Json(channels.into_values().collect::<Vec<_>>()).into_response()
}
