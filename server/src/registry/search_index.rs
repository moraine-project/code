use moraine_model::Canonical;
use moraine_model::profile::ProfileRevision;
use sqlx::Row;

use crate::db::MetadataStore;
use crate::db::sql::SqlBuilder;
use crate::registry::search_labels::loader_version_label;
use crate::routes::AppState;

pub(crate) fn normalize_name(name: &str) -> String {
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
	pub score: i64,
	pub approximate: bool,
}

fn trigram_weights(text: &str, weight: i64, into: &mut std::collections::BTreeMap<String, i64>) {
	for word in text.split(|character: char| !character.is_alphanumeric()) {
		let word = word.to_lowercase();
		if word.is_empty() {
			continue;
		}
		let characters: Vec<char> = word.chars().collect();
		if characters.len() < 3 {
			into.entry(word)
				.and_modify(|existing| *existing = (*existing).max(weight))
				.or_insert(weight);
			continue;
		}
		for window in characters.windows(3) {
			let trigram: String = window.iter().collect();
			into.entry(trigram)
				.and_modify(|existing| *existing = (*existing).max(weight))
				.or_insert(weight);
		}
	}
}

pub(crate) fn query_trigrams(text: &str) -> Vec<String> {
	let mut weights = std::collections::BTreeMap::new();
	trigram_weights(text, 1, &mut weights);
	weights.into_keys().take(80).collect()
}

#[derive(Debug, Clone, Copy)]
pub enum SearchSort {
	Relevance,
	Updated,
	Created,
	Name,
	Popularity,
}

fn score_expression(exact: usize, prefix: usize, substring: usize) -> String {
	format!(
		"(CASE WHEN lower(display_name) = ${exact} THEN 12 WHEN lower(display_name) LIKE ${prefix} ESCAPE '\\' THEN 8 WHEN lower(display_name) LIKE ${substring} ESCAPE '\\' THEN 4 ELSE 0 END + CASE WHEN lower(summary) = ${exact} THEN 6 WHEN lower(summary) LIKE ${prefix} ESCAPE '\\' THEN 4 WHEN lower(summary) LIKE ${substring} ESCAPE '\\' THEN 2 ELSE 0 END + CASE WHEN lower(description) LIKE ${substring} ESCAPE '\\' THEN 1 ELSE 0 END + CASE WHEN EXISTS (SELECT 1 FROM search_changelog_text c WHERE c.project_id = search_documents.project_id AND lower(c.text) LIKE ${substring} ESCAPE '\\') THEN 1 ELSE 0 END)"
	)
}

fn escape_like(text: &str) -> String {
	let mut escaped = String::with_capacity(text.len());
	for character in text.chars() {
		if matches!(character, '\\' | '%' | '_') {
			escaped.push('\\');
		}
		escaped.push(character);
	}
	escaped
}

pub(crate) fn push_match_clauses(query: &mut SqlBuilder, filter: &SearchFilter<'_>, skip: Option<&str>, include_text: bool) {
	if include_text
		&& let Some(escaped) = filter
			.text
			.map(|text| escape_like(text.trim().to_lowercase().as_str()))
			.filter(|text| !text.is_empty())
	{
		let substring = query.reserve_bind(format!("%{escaped}%"));
		query.push(&format!(
			" AND (lower(display_name) LIKE ${substring} ESCAPE '\\' OR lower(summary) LIKE ${substring} ESCAPE '\\' OR lower(description) LIKE ${substring} ESCAPE '\\' OR EXISTS (SELECT 1 FROM search_changelog_text c WHERE c.project_id = search_documents.project_id AND lower(c.text) LIKE ${substring} ESCAPE '\\'))"
		));
	}
	if skip != Some("game")
		&& let Some(game) = filter.game_id
	{
		let parameter = query.reserve_bind(game);
		query.push(&format!(" AND game_id = ${parameter}"));
	}
	for (label_kind, value) in [
		("tag", filter.tag),
		("loader", filter.loader),
		("category", filter.category),
		("game-version", filter.game_version),
		("channel", filter.channel),
		("platform", filter.platform),
		("runtime-version", filter.runtime_version),
	] {
		if skip == Some(label_kind) {
			continue;
		}
		if let Some(value) = value {
			let parameter = query.reserve_bind(value);
			query.push(&format!(" AND EXISTS (SELECT 1 FROM search_labels l WHERE l.project_id = search_documents.project_id AND l.label_kind = '{label_kind}' AND l.label_id = ${parameter})"));
		}
	}
	if skip != Some("loader-version")
		&& let (Some(loader), Some(version)) = (filter.loader, filter.loader_version)
	{
		let parameter = query.reserve_bind(loader_version_label(loader, version));
		query.push(&format!(" AND EXISTS (SELECT 1 FROM search_labels l WHERE l.project_id = search_documents.project_id AND l.label_kind = 'loader-version' AND l.label_id = ${parameter})"));
	}
}

pub struct SearchDocument<'a> {
	pub project_id: &'a str,
	pub game_id: &'a str,
	pub display_name: &'a str,
	pub summary: &'a str,
	pub description: &'a str,
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
	pub game_version: Option<&'a str>,
	pub loader_version: Option<&'a str>,
	pub runtime_version: Option<&'a str>,
	pub channel: Option<&'a str>,
	pub platform: Option<&'a str>,
	pub popularity_since: Option<(i64, i64)>,
	pub sort: SearchSort,
	pub cursor: Option<(&'a str, &'a str)>,
	pub limit: i64,
}

impl MetadataStore {
	pub async fn put_search_document(&self, document: SearchDocument<'_>) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		sqlx::query(
			"INSERT INTO search_documents (project_id, game_id, display_name, summary, description, updated_at, created_at, normalized_name)
			 VALUES ($1, $2, $3, $4, $5, $6, $6, $7)
			 ON CONFLICT(project_id) DO UPDATE SET game_id = $2, display_name = $3, summary = $4, description = $5,
			 updated_at = $6, normalized_name = $7",
		)
		.bind(document.project_id)
		.bind(document.game_id)
		.bind(document.display_name)
		.bind(document.summary)
		.bind(document.description)
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
		sqlx::query("DELETE FROM search_trigrams WHERE project_id = $1")
			.bind(document.project_id)
			.execute(&mut *transaction)
			.await?;
		let mut weights = std::collections::BTreeMap::new();
		trigram_weights(document.display_name, 3, &mut weights);
		trigram_weights(document.summary, 2, &mut weights);
		for (trigram, weight) in &weights {
			sqlx::query(
				"INSERT INTO search_trigrams (trigram, project_id, weight) VALUES ($1, $2, $3)
				 ON CONFLICT(trigram, project_id) DO UPDATE SET weight = $3",
			)
			.bind(trigram)
			.bind(document.project_id)
			.bind(*weight)
			.execute(&mut *transaction)
			.await?;
		}
		transaction.commit().await?;
		Ok(())
	}

	pub async fn fuzzy_candidates(
		&self,
		trigrams: &[String],
		min_shared: i64,
		limit: i64,
	) -> Result<Vec<String>, sqlx::Error> {
		if trigrams.is_empty() {
			return Ok(Vec::new());
		}
		let mut builder = SqlBuilder::new(
			"SELECT project_id, CAST(SUM(weight) AS BIGINT) AS score FROM search_trigrams WHERE trigram IN (",
		);
		{
			let mut separated = builder.separated(", ");
			for trigram in trigrams {
				separated.push_bind(trigram);
			}
		}
		builder.push(") GROUP BY project_id HAVING COUNT(*) >= ");
		builder.push_bind(min_shared);
		builder.push(" ORDER BY score DESC, project_id ASC LIMIT ");
		builder.push_bind(limit);
		let rows = builder.into_query().fetch_all(&self.pool).await?;
		Ok(rows.into_iter().map(|row| row.get("project_id")).collect())
	}

	pub async fn documents_by_id(&self, project_ids: &[String]) -> Result<Vec<SearchHit>, sqlx::Error> {
		if project_ids.is_empty() {
			return Ok(Vec::new());
		}
		let mut builder = SqlBuilder::new(
			"SELECT project_id, game_id, display_name, summary, updated_at, created_at, 0 AS popularity, 0 AS score
			 FROM search_documents WHERE project_id IN (",
		);
		{
			let mut separated = builder.separated(", ");
			for project_id in project_ids {
				separated.push_bind(project_id);
			}
		}
		builder.push(") ORDER BY project_id ASC");
		let rows = builder.into_query().fetch_all(&self.pool).await?;
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
				score: row.get("score"),
				approximate: true,
			})
			.collect())
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

	pub async fn put_changelog_text(&self, object_digest: &[u8], project_id: &str, text: &str) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO search_changelog_text (object_digest, project_id, text) VALUES ($1, $2, $3)
			 ON CONFLICT(object_digest) DO UPDATE SET project_id = $2, text = $3",
		)
		.bind(object_digest)
		.bind(project_id)
		.bind(text)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn search_documents(&self, filter: SearchFilter<'_>) -> Result<Vec<SearchHit>, sqlx::Error> {
		let escaped = filter
			.text
			.map(|text| escape_like(text.trim().to_lowercase().as_str()))
			.filter(|text| !text.is_empty());
		let mut query = SqlBuilder::new("");
		let patterns = escaped.as_deref().map(|text| {
			(
				query.reserve_bind(text),
				query.reserve_bind(format!("{text}%")),
				query.reserve_bind(format!("%{text}%")),
			)
		});
		let relevance = matches!(filter.sort, SearchSort::Relevance) && patterns.is_some();
		let score = match (patterns, relevance) {
			(Some((exact, prefix, substring)), true) => Some(score_expression(exact, prefix, substring)),
			_ => None,
		};
		let mut select =
			String::from("SELECT project_id, game_id, display_name, summary, updated_at, created_at, 0 AS popularity, ");
		match filter.popularity_since {
			Some((day, seconds)) => {
				let day_parameter = query.reserve_bind(day);
				let seconds_parameter = query.reserve_bind(seconds);
				select = format!(
					"WITH pop AS (SELECT project_id, CAST(SUM(count) AS BIGINT) AS total FROM download_counts WHERE day >= ${day_parameter} GROUP BY project_id),
					 fol AS (SELECT project_id, COUNT(*) AS total FROM follows WHERE created_at >= ${seconds_parameter} GROUP BY project_id)
					 SELECT search_documents.project_id, search_documents.game_id, search_documents.display_name,
					 search_documents.summary, search_documents.updated_at, search_documents.created_at,
					 (COALESCE(pop.total, 0) + COALESCE(fol.total, 0)) AS popularity, {score} AS score
					 FROM search_documents
					 LEFT JOIN pop ON pop.project_id = search_documents.project_id
					 LEFT JOIN fol ON fol.project_id = search_documents.project_id WHERE 1 = 1",
					score = score.clone().unwrap_or_else(|| "0".to_string())
				);
			}
			None => {
				select.push_str(&score.clone().unwrap_or_else(|| "0".to_string()));
				select.push_str(" AS score FROM search_documents WHERE 1 = 1");
			}
		}
		query.push(&select);
		if let Some(expression) = &score {
			query.push(&format!(" AND {expression} > 0"));
		}
		push_match_clauses(&mut query, &filter, None, score.is_none());
		match filter.sort {
			SearchSort::Updated => {
				if let Some((value, id)) = filter.cursor {
					let updated = value.parse::<i64>().unwrap_or(i64::MAX);
					let first = query.reserve_bind(updated);
					let second = query.reserve_bind(updated);
					let third = query.reserve_bind(id);
					query.push(&format!(
						" AND (updated_at < ${first} OR (updated_at = ${second} AND project_id < ${third}))"
					));
				}
				query.push(" ORDER BY updated_at DESC, project_id DESC");
			}
			SearchSort::Created => {
				if let Some((value, id)) = filter.cursor {
					let created = value.parse::<i64>().unwrap_or(i64::MAX);
					let first = query.reserve_bind(created);
					let second = query.reserve_bind(created);
					let third = query.reserve_bind(id);
					query.push(&format!(
						" AND (created_at < ${first} OR (created_at = ${second} AND project_id < ${third}))"
					));
				}
				query.push(" ORDER BY created_at DESC, project_id DESC");
			}
			SearchSort::Name => {
				if let Some((value, id)) = filter.cursor {
					let first = query.reserve_bind(value);
					let second = query.reserve_bind(value);
					let third = query.reserve_bind(id);
					query.push(&format!(
						" AND (lower(display_name) > lower(${first}) OR (lower(display_name) = lower(${second}) AND project_id > ${third}))"
					));
				}
				query.push(" ORDER BY lower(display_name) ASC, project_id ASC");
			}
			SearchSort::Relevance => {
				let expression = score.clone().expect("relevance carries a score");
				if let Some((value, id)) = filter.cursor {
					let value = value.parse::<i64>().unwrap_or(i64::MAX);
					let first = query.reserve_bind(value);
					let second = query.reserve_bind(value);
					let third = query.reserve_bind(id);
					query.push(&format!(
						" AND ({expression} < ${first} OR ({expression} = ${second} AND project_id > ${third}))"
					));
				}
				query.push(" ORDER BY score DESC, project_id ASC");
			}
			SearchSort::Popularity => {
				if let Some((value, id)) = filter.cursor {
					let popularity = value.parse::<i64>().unwrap_or(i64::MAX);
					let first = query.reserve_bind(popularity);
					let second = query.reserve_bind(popularity);
					let third = query.reserve_bind(id);
					query.push(&format!(
						" AND ((COALESCE(pop.total, 0) + COALESCE(fol.total, 0)) < ${first}
						 OR ((COALESCE(pop.total, 0) + COALESCE(fol.total, 0)) = ${second} AND search_documents.project_id > ${third}))"
					));
				}
				query.push(" ORDER BY popularity DESC, search_documents.project_id ASC");
			}
		}
		let limit = query.reserve_bind(filter.limit);
		query.push(&format!(" LIMIT ${limit}"));
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
				score: row.get("score"),
				approximate: false,
			})
			.collect())
	}
}

pub(crate) fn validate_profile_vocabulary(
	profile: &ProfileRevision,
	game: &moraine_model::definition::GameDef,
) -> Result<(), String> {
	if !game.categories.is_empty() {
		for category in &profile.categories {
			if !game.categories.iter().any(|declared| &declared.id == category) {
				return Err(format!("`{category}` is not a category of this game"));
			}
		}
	}
	if !game.tags.is_empty() {
		for tag in &profile.tags {
			if !game.tags.iter().any(|declared| &declared.id == tag) {
				return Err(format!("`{tag}` is not a tag of this game"));
			}
		}
	}
	Ok(())
}

pub(crate) async fn game_vocabulary(
	state: &AppState,
	game_id: &str,
) -> Result<Option<moraine_model::definition::GameDef>, sqlx::Error> {
	let Some(definition) = state.metadata.definition(game_id).await? else {
		return Ok(None);
	};
	let Some(current) = definition.current_digest else {
		return Ok(None);
	};
	let Some(object) = state.metadata.object(&current).await? else {
		return Ok(None);
	};
	Ok(moraine_model::definition::GameDef::from_canonical_bytes(&object.payload).ok())
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
			description: &profile.description,
			categories: &profile.categories,
			tags: &profile.tags,
			updated_at: now(),
		})
		.await
}

pub(crate) fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}
