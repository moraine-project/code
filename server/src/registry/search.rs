use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::search::{Annotation, InstancePopularity, ListingState, SearchQuery, SearchResponse, SearchResult};
use serde::Deserialize;

use super::search_index::{SearchFilter, SearchSort, normalize_name, now, query_trigrams};
use crate::routes::AppState;

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
	loader: Option<String>,
	#[serde(default)]
	sort: Option<String>,
	#[serde(default)]
	cursor: Option<String>,
	#[serde(default)]
	limit: Option<u32>,
}

async fn search(State(state): State<AppState>, Query(params): Query<SearchParams>, headers: HeaderMap) -> Response {
	let limit = params.limit.unwrap_or(20).clamp(1, moraine_model::search::MAX_LIMIT) as i64;
	let query_text = params.q.as_deref().filter(|text| !text.trim().is_empty());
	let has_text = query_text.is_some();
	let sort = match params.sort.as_deref() {
		Some("updated") => SearchSort::Updated,
		None | Some("relevance") if has_text => SearchSort::Relevance,
		None | Some("relevance") => SearchSort::Updated,
		Some("name") => SearchSort::Name,
		Some("created") => SearchSort::Created,
		Some("popularity") => SearchSort::Popularity,
		Some(other) => {
			return (
				StatusCode::BAD_REQUEST,
				format!("unsupported sort `{other}`: use relevance, updated, name, created, or popularity"),
			)
				.into_response();
		}
	};
	let since_day = now() / 86_400 - 30;
	let popularity_since = match sort {
		SearchSort::Popularity => Some((since_day, since_day * 86_400)),
		_ => None,
	};
	let cursor = params.cursor.as_deref().and_then(|value| value.rsplit_once(':'));
	let filter = SearchFilter {
		text: query_text,
		game_id: params.game.as_deref(),
		tag: params.tag.as_deref(),
		category: params.category.as_deref(),
		loader: params.loader.as_deref(),
		popularity_since,
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
	let page_tail = hits.last().cloned();

	if hits.len() < limit as usize
		&& let Some(text) = query_text
	{
		let trigrams = query_trigrams(text);
		let min_shared = if trigrams.len() >= 3 { 2 } else { 1 };
		match state.metadata.fuzzy_candidates(&trigrams, min_shared, 20).await {
			Ok(candidates) => {
				let present: std::collections::HashSet<String> = hits.iter().map(|hit| hit.project_id.clone()).collect();
				let wanted: Vec<String> = candidates.into_iter().filter(|id| !present.contains(id)).collect();
				if !wanted.is_empty() {
					match state.metadata.documents_by_id(&wanted).await {
						Ok(mut fuzzy) => {
							hits.append(&mut fuzzy);
							hits.truncate(limit as usize);
						}
						Err(error) => tracing::warn!(%error, "fuzzy search lookup failed"),
					}
				}
			}
			Err(error) => tracing::warn!(%error, "fuzzy search failed"),
		}
	}

	let project_ids: Vec<String> = hits.iter().map(|hit| hit.project_id.clone()).collect();
	let popularities = match state.metadata.popularities(&project_ids, since_day).await {
		Ok(popularities) => popularities,
		Err(error) => {
			tracing::error!(%error, "popularity lookup failed");
			return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
		}
	};
	let homes = match state.metadata.project_homes(&project_ids).await {
		Ok(homes) => homes,
		Err(error) => {
			tracing::error!(%error, "home lookup failed");
			return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
		}
	};
	let source_instance = headers
		.get(header::HOST)
		.and_then(|value| value.to_str().ok())
		.unwrap_or_default()
		.to_string();
	let policies = match state.metadata.listing_policies(&project_ids).await {
		Ok(policies) => policies,
		Err(error) => {
			tracing::error!(%error, "directory policy lookup failed");
			return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
		}
	};
	let keys: Vec<(String, String)> = hits
		.iter()
		.map(|hit| (hit.game_id.clone(), normalize_name(&hit.display_name)))
		.collect();
	let collisions = match state.metadata.name_collision_counts(&keys).await {
		Ok(collisions) => collisions,
		Err(error) => {
			tracing::error!(%error, "name collision lookup failed");
			return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
		}
	};
	let results: Vec<SearchResult> = hits
		.iter()
		.filter_map(|hit| {
			let listing_state = policies
				.get(&hit.project_id)
				.map(|policy| policy.listing_state)
				.unwrap_or(ListingState::Listed);
			if !listing_state.appears_in_search() {
				return None;
			}
			let mut annotations = Vec::new();
			if let Some(count) = collisions
				.get(&(hit.game_id.clone(), normalize_name(&hit.display_name)))
				.filter(|count| **count > 1)
			{
				annotations.push(Annotation {
					kind: "name-collision".to_string(),
					ref_digest: None,
					label: format!(
						"{} projects in this game share this name; compare the IDs, not the names",
						count
					),
				});
			}
			if listing_state == ListingState::Quarantined {
				annotations.push(Annotation {
					kind: "quarantined".to_string(),
					ref_digest: None,
					label: "This instance is holding this project pending a report; do not fetch it automatically"
						.to_string(),
				});
			}
			if hit.approximate {
				annotations.push(Annotation {
					kind: "approximate-match".to_string(),
					ref_digest: None,
					label: "no exact match; this is a near match, so compare the project ID before trusting it".to_string(),
				});
			}
			Some(SearchResult {
				project_id: hit.project_id.clone(),
				game_id: hit.game_id.clone(),
				display_name: hit.display_name.clone(),
				summary: hit.summary.clone(),
				icon_url: None,
				latest_release_id: None,
				matched_release_id: None,
				listing_state,
				source_instance: source_instance.clone(),
				home: homes.get(&hit.project_id).cloned(),
				annotations,
				instance_popularity: popularities.get(&hit.project_id).map(|value| InstancePopularity {
					window: "instance-30d".to_string(),
					value: (*value).max(0) as u64,
				}),
			})
		})
		.collect();
	let next_cursor = if has_more {
		page_tail.map(|hit| match sort {
			SearchSort::Updated => format!("{}:{}", hit.updated_at, hit.project_id),
			SearchSort::Created => format!("{}:{}", hit.created_at, hit.project_id),
			SearchSort::Popularity => format!("{}:{}", hit.popularity, hit.project_id),
			SearchSort::Relevance => format!("{}:{}", hit.score, hit.project_id),
			SearchSort::Name => format!("{}:{}", hit.display_name, hit.project_id),
		})
	} else {
		None
	};
	let query = SearchQuery {
		text: params.q.clone(),
		game: params.game.clone(),
		loader: params.loader.clone(),
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
