pub mod sql;

use std::path::Path;

use sqlx::any::AnyPoolOptions;
use sqlx::{AnyPool, Row, Transaction};

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
		run_migrations(&pool, Engine::of(url)).await?;
		Ok(Self { pool })
	}

	pub async fn put_object(&self, object: &StoredObject) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT INTO objects (digest, kind, payload, wire) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING")
			.bind(&object.digest)
			.bind(&object.kind)
			.bind(&object.payload)
			.bind(&object.wire)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn object(&self, digest: &[u8]) -> Result<Option<StoredObject>, sqlx::Error> {
		let row = sqlx::query("SELECT digest, kind, payload, wire FROM objects WHERE digest = $1")
			.bind(digest)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.map(|row| StoredObject {
			digest: row.get("digest"),
			kind: row.get("kind"),
			payload: row.get("payload"),
			wire: row.get("wire"),
		}))
	}

	pub async fn objects_of_kind(&self, kind: &str, limit: i64) -> Result<Vec<StoredObject>, sqlx::Error> {
		let rows = sqlx::query("SELECT digest, kind, payload, wire FROM objects WHERE kind = $1 LIMIT $2")
			.bind(kind)
			.bind(limit)
			.fetch_all(&self.pool)
			.await?;
		Ok(rows
			.into_iter()
			.map(|row| StoredObject {
				digest: row.get("digest"),
				kind: row.get("kind"),
				payload: row.get("payload"),
				wire: row.get("wire"),
			})
			.collect())
	}

	pub async fn project(&self, id: &str) -> Result<Option<ProjectRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT id, genesis_digest, head_seq, head_digest, profile_digest, owner_kind, owner_id FROM projects WHERE id = $1",
		)
		.bind(id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| ProjectRow {
			id: row.get("id"),
			genesis_digest: row.get("genesis_digest"),
			head_seq: row.get("head_seq"),
			head_digest: row.get("head_digest"),
			profile_digest: row.get("profile_digest"),
			owner_kind: row.get("owner_kind"),
			owner_id: row.get("owner_id"),
		}))
	}

	pub async fn set_project_owner(&self, id: &str, owner_kind: &str, owner_id: &str) -> Result<bool, sqlx::Error> {
		let result = sqlx::query("UPDATE projects SET owner_kind = $1, owner_id = $2 WHERE id = $3")
			.bind(owner_kind)
			.bind(owner_id)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn create_project(&self, id: &str, genesis_digest: &[u8]) -> Result<bool, sqlx::Error> {
		let result = sqlx::query("INSERT INTO projects (id, genesis_digest) VALUES ($1, $2) ON CONFLICT DO NOTHING")
			.bind(id)
			.bind(genesis_digest)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn append_feed(&self, entry: &FeedRow) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		insert_feed_entry(&mut transaction, entry).await?;
		sqlx::query("UPDATE projects SET head_seq = $1, head_digest = $2 WHERE id = $3")
			.bind(entry.seq)
			.bind(&entry.entry_digest)
			.bind(&entry.project_id)
			.execute(&mut *transaction)
			.await?;
		if entry.kind == "profile-updated" {
			sqlx::query("UPDATE projects SET profile_digest = $1 WHERE id = $2")
				.bind(&entry.object_digest)
				.bind(&entry.project_id)
				.execute(&mut *transaction)
				.await?;
		}
		transaction.commit().await?;
		Ok(())
	}

	pub async fn feed_after(&self, project_id: &str, after: i64, limit: i64) -> Result<Vec<FeedRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT project_id, seq, previous, entry_digest, kind, object_digest, payload, wire
			 FROM feed_entries WHERE project_id = $1 AND seq > $2 ORDER BY seq ASC LIMIT $3",
		)
		.bind(project_id)
		.bind(after)
		.bind(limit)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| FeedRow {
				project_id: row.get("project_id"),
				seq: row.get("seq"),
				previous: row.get("previous"),
				entry_digest: row.get("entry_digest"),
				kind: row.get("kind"),
				object_digest: row.get("object_digest"),
				payload: row.get("payload"),
				wire: row.get("wire"),
			})
			.collect())
	}

	pub async fn record_withdrawal(
		&self,
		project_id: &str,
		release_id: &str,
		reason: &str,
		note: Option<&str>,
		declared_time: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO withdrawals (project_id, release_id, reason, note, declared_time) VALUES ($1, $2, $3, $4, $5)
			 ON CONFLICT(project_id, release_id) DO UPDATE SET reason = $3, note = $4, declared_time = $5",
		)
		.bind(project_id)
		.bind(release_id)
		.bind(reason)
		.bind(note)
		.bind(declared_time)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn withdrawal(&self, project_id: &str, release_id: &str) -> Result<Option<WithdrawalRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT release_id, reason, note, declared_time FROM withdrawals WHERE project_id = $1 AND release_id = $2",
		)
		.bind(project_id)
		.bind(release_id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| WithdrawalRow {
			reason: row.get("reason"),
			note: row.get("note"),
			declared_time: row.get("declared_time"),
		}))
	}
}

async fn insert_feed_entry(transaction: &mut Transaction<'_, sqlx::Any>, entry: &FeedRow) -> Result<(), sqlx::Error> {
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

const MIGRATION_LOCK_KEY: i64 = 0x6d6f7261696e65;

struct Migration {
	name: &'static str,
	sqlite: &'static str,
	postgres: &'static str,
}

impl Migration {
	fn sql(&self, engine: Engine) -> &'static str {
		match engine {
			Engine::Sqlite => self.sqlite,
			Engine::Postgres => self.postgres,
		}
	}
}

const MIGRATIONS: &[Migration] = &[Migration {
	name: "0001_initial",
	sqlite: include_str!("../../migrations/sqlite/0001_initial.sql"),
	postgres: include_str!("../../migrations/postgres/0001_initial.sql"),
}];

async fn connect(url: &str, max_connections: u32) -> Result<AnyPool, sqlx::Error> {
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

async fn run_migrations(pool: &AnyPool, engine: Engine) -> Result<usize, sqlx::Error> {
	let mut connection = pool.acquire().await?;
	sqlx::query("CREATE TABLE IF NOT EXISTS schema_migrations (name TEXT PRIMARY KEY, applied_at BIGINT NOT NULL)")
		.execute(&mut *connection)
		.await?;
	let mut applied = 0;
	for migration in MIGRATIONS {
		let begin = match engine {
			Engine::Sqlite => "BEGIN IMMEDIATE",
			Engine::Postgres => "BEGIN",
		};
		sqlx::query(begin).execute(&mut *connection).await?;
		if engine == Engine::Postgres {
			sqlx::query("SELECT pg_advisory_xact_lock($1)")
				.bind(MIGRATION_LOCK_KEY)
				.execute(&mut *connection)
				.await?;
		}
		let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM schema_migrations WHERE name = $1")
			.bind(migration.name)
			.fetch_one(&mut *connection)
			.await?;
		if exists > 0 {
			sqlx::query("ROLLBACK").execute(&mut *connection).await?;
			continue;
		}
		sqlx::raw_sql(migration.sql(engine)).execute(&mut *connection).await?;
		sqlx::query("INSERT INTO schema_migrations (name, applied_at) VALUES ($1, $2) ON CONFLICT(name) DO NOTHING")
			.bind(migration.name)
			.bind(unix_now())
			.execute(&mut *connection)
			.await?;
		sqlx::query("COMMIT").execute(&mut *connection).await?;
		applied += 1;
	}
	Ok(applied)
}

pub async fn migrate_url(url: &str) -> Result<usize, sqlx::Error> {
	let pool = connect(url, 1).await?;
	run_migrations(&pool, Engine::of(url)).await
}

pub async fn pending_url(url: &str) -> Result<usize, sqlx::Error> {
	let engine = Engine::of(url);
	if engine == Engine::Sqlite {
		let path = url.trim_start_matches("sqlite:").split('?').next().unwrap_or_default();
		if !Path::new(path).exists() {
			return Ok(MIGRATIONS.len());
		}
	}
	let pool = connect(url, 1).await?;
	let tracked = match engine {
		Engine::Sqlite => {
			sqlx::query_scalar::<_, i64>(
				"SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
			)
			.fetch_one(&pool)
			.await?
		}
		Engine::Postgres => {
			sqlx::query_scalar::<_, i64>(
				"SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'schema_migrations'",
			)
			.fetch_one(&pool)
			.await?
		}
	};
	if tracked == 0 {
		return Ok(MIGRATIONS.len());
	}
	let applied = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM schema_migrations")
		.fetch_one(&pool)
		.await?;
	Ok(MIGRATIONS.len().saturating_sub(applied as usize))
}

fn unix_now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|elapsed| elapsed.as_secs() as i64)
		.unwrap_or(0)
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
		use crate::registry::search::{SearchDocument, SearchFilter, SearchSort};

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
mod migration_tests {
	use super::*;

	#[tokio::test]
	async fn migrations_are_pending_until_applied() {
		let directory = tempfile::tempdir().expect("tempdir");
		let url = sqlite_url(&directory.path().join("metadata.sqlite"));

		assert_eq!(pending_url(&url).await.expect("pending"), MIGRATIONS.len());

		assert_eq!(migrate_url(&url).await.expect("migrate"), MIGRATIONS.len());
		assert_eq!(pending_url(&url).await.expect("pending"), 0);
		assert_eq!(migrate_url(&url).await.expect("migrate again"), 0);
	}

	#[tokio::test]
	async fn concurrent_openers_apply_migrations_once() {
		let directory = tempfile::tempdir().expect("tempdir");
		let path = directory.path().join("metadata.sqlite");

		let (first, second) = tokio::join!(MetadataStore::open(&path), MetadataStore::open(&path));

		assert!(first.is_ok(), "{:?}", first.err());
		assert!(second.is_ok(), "{:?}", second.err());
		assert_eq!(pending_url(&sqlite_url(&path)).await.expect("pending"), 0);
	}
}

#[cfg(test)]
mod postgres_tests {
	use super::*;
	use crate::registry::review::{ReviewDecisionRow, SubmissionRow};
	use crate::registry::search::SearchDocument;

	async fn admin_pool(url: &str) -> AnyPool {
		sqlx::any::install_default_drivers();
		AnyPoolOptions::new()
			.max_connections(1)
			.connect(url)
			.await
			.expect("admin pool")
	}

	#[tokio::test]
	async fn round_trips_through_postgres() {
		let Ok(url) = std::env::var("MORAINE_TEST_POSTGRES") else {
			return;
		};
		assert!(url.contains("test"), "refusing to reset a non-test database: {url}");

		let pool = admin_pool(&url).await;
		sqlx::raw_sql("DROP SCHEMA public CASCADE; CREATE SCHEMA public")
			.execute(&pool)
			.await
			.expect("reset schema");
		drop(pool);

		let store = MetadataStore::open_url(&url).await.expect("open");
		assert_eq!(pending_url(&url).await.expect("pending"), 0);

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
