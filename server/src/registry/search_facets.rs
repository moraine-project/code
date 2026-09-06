use serde::Serialize;
use sqlx::Row;

use super::search_index::{SearchFilter, push_match_clauses};
use crate::db::MetadataStore;
use crate::db::sql::SqlBuilder;

const FACET_LIMIT: i64 = 50;

#[derive(Debug, Clone, Serialize)]
pub struct FacetValue {
	pub value: String,
	pub count: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Facets {
	pub game: Vec<FacetValue>,
	pub loader: Vec<FacetValue>,
	pub category: Vec<FacetValue>,
	pub tag: Vec<FacetValue>,
	pub game_version: Vec<FacetValue>,
	pub loader_version: Vec<FacetValue>,
	pub runtime_version: Vec<FacetValue>,
	pub channel: Vec<FacetValue>,
	pub platform: Vec<FacetValue>,
}

impl MetadataStore {
	pub async fn search_facets(&self, filter: &SearchFilter<'_>) -> Result<Facets, sqlx::Error> {
		Ok(Facets {
			game: self.facet_by_game(filter).await?,
			loader: self.facet_by_label(filter, "loader").await?,
			category: self.facet_by_label(filter, "category").await?,
			tag: self.facet_by_label(filter, "tag").await?,
			game_version: self.facet_by_label(filter, "game-version").await?,
			loader_version: self.facet_by_label(filter, "loader-version").await?,
			runtime_version: self.facet_by_label(filter, "runtime-version").await?,
			channel: self.facet_by_label(filter, "channel").await?,
			platform: self.facet_by_label(filter, "platform").await?,
		})
	}

	async fn facet_by_game(&self, filter: &SearchFilter<'_>) -> Result<Vec<FacetValue>, sqlx::Error> {
		let mut query = SqlBuilder::new("SELECT game_id AS value, COUNT(*) AS total FROM search_documents WHERE 1 = 1");
		push_match_clauses(&mut query, filter, Some("game"), true);
		query.push(" GROUP BY game_id ORDER BY total DESC, value ASC LIMIT ");
		query.push_bind(FACET_LIMIT);
		let rows = query.into_query().fetch_all(&self.pool).await?;
		Ok(rows
			.into_iter()
			.map(|row| FacetValue {
				value: row.get("value"),
				count: row.get("total"),
			})
			.collect())
	}

	async fn facet_by_label(&self, filter: &SearchFilter<'_>, label_kind: &str) -> Result<Vec<FacetValue>, sqlx::Error> {
		if label_kind == "loader-version" && filter.loader.is_none() {
			return Ok(Vec::new());
		}
		let mut query = SqlBuilder::new(
			"SELECT l.label_id AS value, COUNT(*) AS total FROM search_labels l
			 JOIN search_documents ON search_documents.project_id = l.project_id WHERE l.label_kind = ",
		);
		let kind = query.reserve_bind(label_kind);
		query.push(&format!("${kind}"));
		push_match_clauses(&mut query, filter, Some(label_kind), true);
		query.push(" GROUP BY l.label_id ORDER BY total DESC, value ASC LIMIT ");
		query.push_bind(FACET_LIMIT);
		let rows = query.into_query().fetch_all(&self.pool).await?;
		Ok(rows
			.into_iter()
			.map(|row| FacetValue {
				value: row.get("value"),
				count: row.get("total"),
			})
			.collect())
	}
}
