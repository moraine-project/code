pub mod migrations;
pub mod sql;

mod objects;
mod projects;

use std::path::Path;

use sqlx::any::AnyPoolOptions;
use sqlx::{AnyPool, Transaction};

#[derive(Debug, Clone)]
pub struct StoredObject {
	pub digest: Vec<u8>,
	pub kind: String,
	pub payload: Vec<u8>,
	pub wire: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ProjectRow {
	pub id: String,
	pub genesis_digest: Vec<u8>,
	pub head_seq: i64,
	pub head_digest: Option<Vec<u8>>,
	pub profile_digest: Option<Vec<u8>>,
	pub owner_kind: Option<String>,
	pub owner_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FeedRow {
	pub project_id: String,
	pub seq: i64,
	pub previous: Option<Vec<u8>>,
	pub entry_digest: Vec<u8>,
	pub kind: String,
	pub object_digest: Vec<u8>,
	pub payload: Vec<u8>,
	pub wire: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct WithdrawalRow {
	pub reason: String,
	pub note: Option<String>,
	pub declared_time: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
	Sqlite,
	Postgres,
}

impl Engine {
	pub fn of(url: &str) -> Self {
		if url.starts_with("postgres://") || url.starts_with("postgresql://") {
			Self::Postgres
		} else {
			Self::Sqlite
		}
	}
}

pub(crate) fn sqlite_path(url: &str) -> Option<&Path> {
	let path = url.strip_prefix("sqlite:")?.split('?').next()?;
	Some(Path::new(path))
}

pub fn sqlite_url(path: &Path) -> String {
	format!("sqlite:{}?mode=rwc", path.display())
}

pub struct MetadataStore {
	pub(crate) pool: AnyPool,
}

impl MetadataStore {
	pub async fn open(path: impl AsRef<Path>) -> Result<Self, sqlx::Error> {
		if let Some(parent) = path.as_ref().parent() {
			std::fs::create_dir_all(parent).map_err(sqlx::Error::Io)?;
		}
		Self::open_url(&sqlite_url(path.as_ref())).await
	}

	pub async fn open_url(url: &str) -> Result<Self, sqlx::Error> {
		let pool = connect(url, 5).await?;
		migrations::run_migrations(&pool, Engine::of(url)).await?;
		Ok(Self { pool })
	}
}

pub(super) async fn insert_feed_entry(
	transaction: &mut Transaction<'_, sqlx::Any>,
	entry: &FeedRow,
) -> Result<(), sqlx::Error> {
	sqlx::query(
		"INSERT INTO feed_entries (project_id, seq, previous, entry_digest, kind, object_digest, payload, wire)
		 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
	)
	.bind(&entry.project_id)
	.bind(entry.seq)
	.bind(&entry.previous)
	.bind(&entry.entry_digest)
	.bind(&entry.kind)
	.bind(&entry.object_digest)
	.bind(&entry.payload)
	.bind(&entry.wire)
	.execute(&mut **transaction)
	.await?;
	Ok(())
}

impl MetadataStore {
	pub(crate) async fn table_count(&self, table: &str) -> Result<i64, sqlx::Error> {
		self.scalar_count(&format!("SELECT COUNT(*) FROM {table}")).await
	}

	pub(crate) async fn scalar_count(&self, query: &str) -> Result<i64, sqlx::Error> {
		sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(query))
			.fetch_one(&self.pool)
			.await
	}

	pub(crate) async fn scalar_opt(&self, query: &str) -> Result<Option<i64>, sqlx::Error> {
		sqlx::query_scalar::<_, Option<i64>>(sqlx::AssertSqlSafe(query))
			.fetch_one(&self.pool)
			.await
	}
}

const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub(crate) async fn connect(url: &str, max_connections: u32) -> Result<AnyPool, sqlx::Error> {
	sqlx::any::install_default_drivers();
	let engine = Engine::of(url);
	AnyPoolOptions::new()
		.max_connections(max_connections)
		.acquire_timeout(CONNECT_TIMEOUT)
		.after_connect(move |connection, _| {
			Box::pin(async move {
				if engine == Engine::Sqlite {
					sqlx::query("PRAGMA foreign_keys = ON").execute(&mut *connection).await?;
					sqlx::query("PRAGMA busy_timeout = 10000").execute(&mut *connection).await?;
				}
				Ok(())
			})
		})
		.connect(url)
		.await
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn stores_objects_projects_and_feed_with_head_updates() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		let digest = vec![1u8; 32];
		store
			.put_object(&StoredObject {
				digest: digest.clone(),
				kind: "genesis".to_string(),
				payload: vec![0xaa],
				wire: vec![0xbb],
			})
			.await
			.expect("put");
		assert!(store.object(&digest).await.expect("get").is_some());

		assert!(store.create_project("p", &digest).await.expect("create"));
		assert!(!store.create_project("p", &digest).await.expect("duplicate"));

		let entry = FeedRow {
			project_id: "p".to_string(),
			seq: 1,
			previous: None,
			entry_digest: vec![2u8; 32],
			kind: "profile-updated".to_string(),
			object_digest: vec![3u8; 32],
			payload: vec![0xcc],
			wire: vec![0xdd],
		};
		store.append_feed(&entry).await.expect("append");
		let project = store.project("p").await.expect("project").expect("present");
		assert_eq!(project.head_seq, 1);
		assert_eq!(project.profile_digest, Some(vec![3u8; 32]));
		let feed = store.feed_after("p", 0, 10).await.expect("feed");
		assert_eq!(feed.len(), 1);

		store.index_artifact(&[1u8; 32], "p", &[2u8; 32]).await.expect("index");
		let matches = store.artifacts_for_digest(&[1u8; 32]).await.expect("lookup");
		assert_eq!(matches.len(), 1);
	}

	#[tokio::test]
	async fn popularity_counts_downloads_and_follows_in_the_window() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		store.index_artifact(&[1u8; 32], "p", &[2u8; 32]).await.expect("index");
		store.record_download(&[1u8; 32], 100).await.expect("download");
		store.record_download(&[1u8; 32], 100).await.expect("download");
		store.follow("u", "p", 100 * 86_400 + 10).await.expect("follow");
		store.follow("v", "p", 1).await.expect("old follow");
		store.index_artifact(&[3u8; 32], "q", &[2u8; 32]).await.expect("index");

		let totals = store
			.popularities(&["p".to_string(), "q".to_string()], 100)
			.await
			.expect("popularity");

		assert_eq!(totals.get("p"), Some(&3));
		assert_eq!(totals.get("q"), None);
	}
	#[tokio::test]
	async fn popularity_sort_queries() {
		use crate::registry::search_index::{SearchDocument, SearchFilter, SearchSort};

		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		store
			.put_search_document(SearchDocument {
				project_id: "p",
				game_id: "g",
				display_name: "P",
				summary: "s",
				description: "a longer description",
				categories: &[],
				tags: &[],
				updated_at: 1,
			})
			.await
			.expect("doc");
		store.index_artifact(&[1u8; 32], "p", &[2u8; 32]).await.expect("index");
		store.record_download(&[1u8; 32], 100).await.expect("download");
		let hits = store
			.search_documents(SearchFilter {
				text: None,
				game_id: None,
				tag: None,
				category: None,
				loader: None,
				game_version: None,
				loader_version: None,
				runtime_version: None,
				channel: None,
				platform: None,
				popularity_since: Some((100, 100 * 86_400)),
				sort: SearchSort::Popularity,
				cursor: None,
				limit: 10,
			})
			.await;
		assert!(hits.is_ok(), "{:?}", hits.err());
	}
}

#[cfg(test)]
mod postgres_tests {
	use super::*;
	use crate::registry::review::{ReviewDecisionRow, SubmissionRow};
	use crate::registry::search_index::SearchDocument;

	#[tokio::test]
	async fn round_trips_through_postgres() {
		let Some(store) = crate::test_support::isolated_store().await else {
			return;
		};

		store
			.put_object(&StoredObject {
				digest: vec![1u8; 32],
				kind: "release".to_string(),
				payload: vec![2u8; 8],
				wire: vec![3u8; 8],
			})
			.await
			.expect("put object");
		assert_eq!(
			store.object(&[1u8; 32]).await.expect("object").expect("present").payload,
			vec![2u8; 8]
		);

		store.create_project("p", &[1u8; 32]).await.expect("project");
		store
			.append_feed(&FeedRow {
				project_id: "p".to_string(),
				seq: 1,
				previous: None,
				entry_digest: vec![4u8; 32],
				kind: "release-published".to_string(),
				object_digest: vec![1u8; 32],
				payload: vec![5u8; 4],
				wire: vec![6u8; 4],
			})
			.await
			.expect("append");
		assert_eq!(store.feed_after("p", 0, 10).await.expect("feed").len(), 1);

		store.index_artifact(&[1u8; 32], "p", &[7u8; 32]).await.expect("index");
		store.record_download(&[1u8; 32], 100).await.expect("download");
		let popularities = store.popularities(&["p".to_string()], 0).await.expect("popularities");
		assert_eq!(popularities.get("p"), Some(&1));

		store
			.create_submission(&SubmissionRow {
				id: "s".to_string(),
				project_id: "p".to_string(),
				object_digest: vec![1u8; 32],
				entry_digest: vec![4u8; 32],
				entry_wire: vec![6u8; 4],
				state: "submitted".to_string(),
				assigned_to: None,
				submitted_by: "u".to_string(),
				created_at: 1,
				updated_at: 1,
			})
			.await
			.expect("submission");
		assert!(store.assign_submission("s", "r", 2).await.expect("assign"));
		store
			.insert_decision(&ReviewDecisionRow {
				id: "d".to_string(),
				submission_id: "s".to_string(),
				object_digest: vec![1u8; 32],
				reviewer_id: "r".to_string(),
				decision: "reject".to_string(),
				reason_code: Some("spam".to_string()),
				reason_taxonomy_version: 1,
				decided_at: 3,
				appeal_route: None,
			})
			.await
			.expect("decision");
		assert_eq!(store.decisions_for("s").await.expect("decisions").len(), 1);

		store
			.put_search_document(SearchDocument {
				project_id: "p",
				game_id: "g",
				display_name: "Example",
				summary: "s",
				description: "a longer description",
				categories: &[],
				tags: &[],
				updated_at: 1,
			})
			.await
			.expect("document");
		let collision = store
			.name_collision_counts(&[("g".to_string(), "example".to_string())])
			.await
			.expect("collisions");
		assert_eq!(collision.get(&("g".to_string(), "example".to_string())), Some(&1));

		let snapshot = store.metrics_snapshot().await.expect("metrics");
		assert_eq!(snapshot.projects, 1);
		assert_eq!(snapshot.artifacts, 1);
	}
}
