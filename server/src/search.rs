use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::Canonical;
use moraine_model::profile::ProfileRevision;
use moraine_model::search::{ListingState, SearchQuery, SearchResponse, SearchResult};
use serde::Deserialize;

use crate::routes::AppState;
use crate::store::{SearchDocument, SearchFilter, SearchSort};

pub fn routes() -> Router<AppState> {
	Router::new().route("/v1/search", get(search))
}

#[derive(Deserialize)]
struct SearchParams {
	#[serde(default)]
	q: Option<String>,
	#[serde(default)]
	game: Option<String>,
	#[serde(default)]
	tag: Option<String>,
	#[serde(default)]
	category: Option<String>,
	#[serde(default)]
	sort: Option<String>,
	#[serde(default)]
	cursor: Option<String>,
	#[serde(default)]
	limit: Option<u32>,
}

async fn search(State(state): State<AppState>, Query(params): Query<SearchParams>, headers: HeaderMap) -> Response {
	let limit = params.limit.unwrap_or(20).clamp(1, moraine_model::search::MAX_LIMIT) as i64;
	let sort = if params.sort.as_deref() == Some("name") {
		SearchSort::Name
	} else {
		SearchSort::Updated
	};
	let cursor = params.cursor.as_deref().and_then(|value| value.rsplit_once(':'));
	let filter = SearchFilter {
		text: params.q.as_deref().filter(|text| !text.trim().is_empty()),
		game_id: params.game.as_deref(),
		tag: params.tag.as_deref(),
		category: params.category.as_deref(),
		sort,
		cursor,
		limit: limit + 1,
	};
	let mut hits = match state.metadata.search_documents(filter).await {
		Ok(hits) => hits,
		Err(error) => {
			tracing::error!(%error, "search failed");
			return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
		}
	};
	let has_more = hits.len() as i64 > limit;
	hits.truncate(limit as usize);

	let source_instance = headers
		.get(header::HOST)
		.and_then(|value| value.to_str().ok())
		.unwrap_or_default()
		.to_string();
	let results: Vec<SearchResult> = hits
		.iter()
		.map(|hit| SearchResult {
			project_id: hit.project_id.clone(),
			game_id: hit.game_id.clone(),
			display_name: hit.display_name.clone(),
			summary: hit.summary.clone(),
			icon_url: None,
			latest_release_id: None,
			matched_release_id: None,
			listing_state: ListingState::Listed,
			source_instance: source_instance.clone(),
			annotations: Vec::new(),
			instance_popularity: None,
		})
		.collect();
	let next_cursor = if has_more {
		hits.last().map(|hit| match sort {
			SearchSort::Updated => format!("{}:{}", hit.updated_at, hit.project_id),
			SearchSort::Name => format!("{}:{}", hit.display_name, hit.project_id),
		})
	} else {
		None
	};
	let query = SearchQuery {
		text: params.q.clone(),
		game: params.game.clone(),
		loader: None,
		category: params.category.clone().map(|category| vec![category]),
		tag: params.tag.clone().map(|tag| vec![tag]),
		game_version: None,
		sort: params.sort.clone(),
		cursor: params.cursor.clone(),
		limit: limit as u32,
	};
	Json(SearchResponse {
		protocol: 1,
		query,
		results,
		next_cursor,
		total_estimate: None,
	})
	.into_response()
}

pub(crate) async fn refresh_search_document(state: &AppState, object_digest: &[u8]) -> Result<(), sqlx::Error> {
	let Some(object) = state.metadata.object(object_digest).await? else {
		return Ok(());
	};
	let Ok(profile) = ProfileRevision::from_canonical_bytes(&object.payload) else {
		return Ok(());
	};
	state
		.metadata
		.put_search_document(SearchDocument {
			project_id: &profile.project_id,
			game_id: &profile.game_id,
			display_name: &profile.display_name,
			summary: &profile.summary,
			categories: &profile.categories,
			tags: &profile.tags,
			updated_at: now(),
		})
		.await
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}
