use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::Canonical;
use moraine_model::profile::ProfileRevision;
use moraine_model::search::{Annotation, InstancePopularity, ListingState, SearchQuery, SearchResponse, SearchResult};
use serde::Deserialize;
use sqlx::Row;

use crate::routes::AppState;
use crate::sql::SqlBuilder;
use crate::store::MetadataStore;

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
	let sort = match params.sort.as_deref() {
		None | Some("relevance") | Some("updated") => SearchSort::Updated,
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
		text: params.q.as_deref().filter(|text| !text.trim().is_empty()),
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
			home: homes.get(&hit.project_id).cloned(),
			annotations: collisions
				.get(&(hit.game_id.clone(), normalize_name(&hit.display_name)))
				.filter(|count| **count > 1)
				.map(|count| {
					vec![Annotation {
						kind: "name-collision".to_string(),
						ref_digest: None,
						label: format!(
							"{} projects in this game share this name; compare the IDs, not the names",
							count
						),
					}]
				})
				.unwrap_or_default(),
			instance_popularity: popularities.get(&hit.project_id).map(|value| InstancePopularity {
				window: "instance-30d".to_string(),
				value: (*value).max(0) as u64,
			}),
		})
		.collect();
	let next_cursor = if has_more {
		hits.last().map(|hit| match sort {
			SearchSort::Updated => format!("{}:{}", hit.updated_at, hit.project_id),
			SearchSort::Created => format!("{}:{}", hit.created_at, hit.project_id),
			SearchSort::Popularity => format!("{}:{}", hit.popularity, hit.project_id),
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

fn normalize_name(name: &str) -> String {
	name.chars()
		.filter(|character| character.is_alphanumeric())
		.flat_map(|character| character.to_lowercase())
		.collect()
}

#[derive(Debug, Clone)]
pub struct SearchHit {
	pub project_id: String,
	pub game_id: String,
	pub display_name: String,
	pub summary: String,
	pub updated_at: i64,
	pub created_at: i64,
	pub popularity: i64,
}

#[derive(Debug, Clone, Copy)]
pub enum SearchSort {
	Updated,
	Created,
	Name,
	Popularity,
}

pub struct SearchDocument<'a> {
	pub project_id: &'a str,
	pub game_id: &'a str,
	pub display_name: &'a str,
	pub summary: &'a str,
	pub categories: &'a [String],
	pub tags: &'a [String],
	pub updated_at: i64,
}

pub struct SearchFilter<'a> {
	pub text: Option<&'a str>,
	pub game_id: Option<&'a str>,
	pub tag: Option<&'a str>,
	pub category: Option<&'a str>,
	pub loader: Option<&'a str>,
	pub popularity_since: Option<(i64, i64)>,
	pub sort: SearchSort,
	pub cursor: Option<(&'a str, &'a str)>,
	pub limit: i64,
}

impl MetadataStore {
	pub async fn put_search_document(&self, document: SearchDocument<'_>) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		sqlx::query(
			"INSERT INTO search_documents (project_id, game_id, display_name, summary, updated_at, created_at, normalized_name)
			 VALUES ($1, $2, $3, $4, $5, $5, $6)
			 ON CONFLICT(project_id) DO UPDATE SET game_id = $2, display_name = $3, summary = $4, updated_at = $5, normalized_name = $6",
		)
		.bind(document.project_id)
		.bind(document.game_id)
		.bind(document.display_name)
		.bind(document.summary)
		.bind(document.updated_at)
		.bind(normalize_name(document.display_name))
		.execute(&mut *transaction)
		.await?;
		sqlx::query("DELETE FROM search_labels WHERE project_id = $1 AND label_kind IN ('category', 'tag')")
			.bind(document.project_id)
			.execute(&mut *transaction)
			.await?;
		for (label_kind, labels) in [("category", document.categories), ("tag", document.tags)] {
			for label in labels {
				sqlx::query(
					"INSERT INTO search_labels (project_id, label_kind, label_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
				)
				.bind(document.project_id)
				.bind(label_kind)
				.bind(label)
				.execute(&mut *transaction)
				.await?;
			}
		}
		transaction.commit().await?;
		Ok(())
	}

	pub async fn project_homes(
		&self,
		project_ids: &[String],
	) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
		let mut homes = std::collections::HashMap::new();
		if project_ids.is_empty() {
			return Ok(homes);
		}
		let mut builder = SqlBuilder::new("SELECT project_id, home_url FROM subscriptions WHERE project_id IN (");
		{
			let mut separated = builder.separated(", ");
			for project_id in project_ids {
				separated.push_bind(project_id);
			}
		}
		builder.push(
			") AND updated_at = (SELECT MAX(updated_at) FROM subscriptions WHERE project_id = subscriptions.project_id)
			 ORDER BY home_url",
		);
		for row in builder.into_query().fetch_all(&self.pool).await? {
			homes.entry(row.get("project_id")).or_insert_with(|| row.get("home_url"));
		}
		Ok(homes)
	}

	pub async fn name_collision_counts(
		&self,
		keys: &[(String, String)],
	) -> Result<std::collections::HashMap<(String, String), i64>, sqlx::Error> {
		let mut counts = std::collections::HashMap::new();
		if keys.is_empty() {
			return Ok(counts);
		}
		let mut builder = SqlBuilder::new(
			"SELECT game_id, normalized_name, COUNT(*) AS total FROM search_documents WHERE (game_id, normalized_name) IN (",
		);
		{
			let mut separated = builder.separated(", ");
			for (game_id, name) in keys {
				separated
					.push("(")
					.push_bind_unseparated(game_id)
					.push_unseparated(", ")
					.push_bind_unseparated(name)
					.push_unseparated(")");
			}
		}
		builder.push(") GROUP BY game_id, normalized_name");
		for row in builder.into_query().fetch_all(&self.pool).await? {
			counts.insert((row.get("game_id"), row.get("normalized_name")), row.get("total"));
		}
		Ok(counts)
	}

	pub async fn add_search_labels(&self, project_id: &str, label_kind: &str, labels: &[String]) -> Result<(), sqlx::Error> {
		for label in labels {
			sqlx::query(
				"INSERT INTO search_labels (project_id, label_kind, label_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
			)
			.bind(project_id)
			.bind(label_kind)
			.bind(label)
			.execute(&self.pool)
			.await?;
		}
		Ok(())
	}

	pub async fn search_documents(&self, filter: SearchFilter<'_>) -> Result<Vec<SearchHit>, sqlx::Error> {
		let mut query = match filter.popularity_since {
			Some((day, seconds)) => {
				let mut query = SqlBuilder::new(
					"WITH pop AS (SELECT project_id, CAST(SUM(count) AS BIGINT) AS total FROM download_counts WHERE day >= ",
				);
				query
					.push_bind(day)
					.push(
						" GROUP BY project_id), fol AS (SELECT project_id, COUNT(*) AS total FROM follows WHERE created_at >= ",
					)
					.push_bind(seconds)
					.push(
						" GROUP BY project_id) SELECT search_documents.project_id, search_documents.game_id,
						 search_documents.display_name, search_documents.summary, search_documents.updated_at,
						 search_documents.created_at, (COALESCE(pop.total, 0) + COALESCE(fol.total, 0)) AS popularity
						 FROM search_documents
						 LEFT JOIN pop ON pop.project_id = search_documents.project_id
						 LEFT JOIN fol ON fol.project_id = search_documents.project_id WHERE 1 = 1",
					);
				query
			}
			None => SqlBuilder::new(
				"SELECT project_id, game_id, display_name, summary, updated_at, created_at, 0 AS popularity FROM search_documents WHERE 1 = 1",
			),
		};
		if let Some(text) = filter.text {
			let pattern = format!("%{}%", text.to_lowercase());
			query
				.push(" AND (lower(display_name) LIKE ")
				.push_bind(pattern.clone())
				.push(" OR lower(summary) LIKE ")
				.push_bind(pattern)
				.push(")");
		}
		if let Some(game) = filter.game_id {
			query.push(" AND game_id = ").push_bind(game);
		}
		if let Some(tag) = filter.tag {
			query
				.push(" AND EXISTS (SELECT 1 FROM search_labels l WHERE l.project_id = search_documents.project_id AND l.label_kind = 'tag' AND l.label_id = ")
				.push_bind(tag)
				.push(")");
		}
		if let Some(loader) = filter.loader {
			query
				.push(" AND EXISTS (SELECT 1 FROM search_labels l WHERE l.project_id = search_documents.project_id AND l.label_kind = 'loader' AND l.label_id = ")
				.push_bind(loader)
				.push(")");
		}
		if let Some(category) = filter.category {
			query
				.push(" AND EXISTS (SELECT 1 FROM search_labels l WHERE l.project_id = search_documents.project_id AND l.label_kind = 'category' AND l.label_id = ")
				.push_bind(category)
				.push(")");
		}
		match filter.sort {
			SearchSort::Updated => {
				if let Some((value, id)) = filter.cursor {
					let updated = value.parse::<i64>().unwrap_or(i64::MAX);
					query
						.push(" AND (updated_at < ")
						.push_bind(updated)
						.push(" OR (updated_at = ")
						.push_bind(updated)
						.push(" AND project_id < ")
						.push_bind(id)
						.push("))");
				}
				query.push(" ORDER BY updated_at DESC, project_id DESC");
			}
			SearchSort::Created => {
				if let Some((value, id)) = filter.cursor {
					let created = value.parse::<i64>().unwrap_or(i64::MAX);
					query
						.push(" AND (created_at < ")
						.push_bind(created)
						.push(" OR (created_at = ")
						.push_bind(created)
						.push(" AND project_id < ")
						.push_bind(id)
						.push("))");
				}
				query.push(" ORDER BY created_at DESC, project_id DESC");
			}
			SearchSort::Name => {
				if let Some((value, id)) = filter.cursor {
					query
						.push(" AND (lower(display_name) > lower(")
						.push_bind(value.to_string())
						.push(") OR (lower(display_name) = lower(")
						.push_bind(value.to_string())
						.push(") AND project_id > ")
						.push_bind(id)
						.push("))");
				}
				query.push(" ORDER BY lower(display_name) ASC, project_id ASC");
			}
			SearchSort::Popularity => {
				const POPULARITY: &str = "(COALESCE(pop.total, 0) + COALESCE(fol.total, 0))";
				if let Some((value, id)) = filter.cursor {
					let popularity = value.parse::<i64>().unwrap_or(i64::MAX);
					query
						.push(" AND (")
						.push(POPULARITY)
						.push(" < ")
						.push_bind(popularity)
						.push(" OR (")
						.push(POPULARITY)
						.push(" = ")
						.push_bind(popularity)
						.push(" AND search_documents.project_id > ")
						.push_bind(id)
						.push("))");
				}
				query.push(" ORDER BY popularity DESC, search_documents.project_id ASC");
			}
		}
		query.push(" LIMIT ").push_bind(filter.limit);
		let rows = query.into_query().fetch_all(&self.pool).await?;
		Ok(rows
			.into_iter()
			.map(|row| SearchHit {
				project_id: row.get("project_id"),
				game_id: row.get("game_id"),
				display_name: row.get("display_name"),
				summary: row.get("summary"),
				updated_at: row.get("updated_at"),
				created_at: row.get("created_at"),
				popularity: row.get("popularity"),
			})
			.collect())
	}
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

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn counts_same_names_across_the_whole_index() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		for (id, name, game) in [
			("a", "Example Mod", "game"),
			("b", "example-mod", "game"),
			("c", "Example Mod", "other"),
		] {
			store
				.put_search_document(SearchDocument {
					project_id: id,
					game_id: game,
					display_name: name,
					summary: "",
					categories: &[],
					tags: &[],
					updated_at: 0,
				})
				.await
				.expect("document");
		}

		let counts = store
			.name_collision_counts(&[
				("game".to_string(), "examplemod".to_string()),
				("other".to_string(), "examplemod".to_string()),
				("game".to_string(), "unrelated".to_string()),
			])
			.await
			.expect("counts");

		assert_eq!(counts.get(&("game".to_string(), "examplemod".to_string())), Some(&2));
		assert_eq!(counts.get(&("other".to_string(), "examplemod".to_string())), Some(&1));
		assert_eq!(counts.get(&("game".to_string(), "unrelated".to_string())), None);
	}
}

#[cfg(test)]
mod home_tests {
	use super::*;

	#[tokio::test]
	async fn reports_the_home_a_project_was_followed_from() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		store
			.upsert_subscription("https://older.example", "p", "active", 1)
			.await
			.expect("subscribe");
		store
			.upsert_subscription("https://newer.example", "p", "active", 5)
			.await
			.expect("subscribe");

		let homes = store.project_homes(&["p".to_string(), "q".to_string()]).await.expect("homes");

		assert_eq!(homes.get("p"), Some(&"https://newer.example".to_string()));
		assert_eq!(homes.get("q"), None);
	}
}
