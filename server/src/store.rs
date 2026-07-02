use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool, Transaction};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS objects (
	digest BLOB PRIMARY KEY,
	kind TEXT NOT NULL,
	payload BLOB NOT NULL,
	wire BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS projects (
	id TEXT PRIMARY KEY,
	genesis_digest BLOB NOT NULL,
	head_seq INTEGER NOT NULL DEFAULT 0,
	head_digest BLOB,
	profile_digest BLOB
);
CREATE TABLE IF NOT EXISTS feed_entries (
	project_id TEXT NOT NULL,
	seq INTEGER NOT NULL,
	previous BLOB,
	entry_digest BLOB NOT NULL,
	kind TEXT NOT NULL,
	object_digest BLOB NOT NULL,
	payload BLOB NOT NULL,
	wire BLOB NOT NULL,
	PRIMARY KEY (project_id, seq)
);
";

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

pub struct MetadataStore {
	pool: SqlitePool,
}

impl MetadataStore {
	pub async fn open(path: impl AsRef<Path>) -> Result<Self, sqlx::Error> {
		let options = SqliteConnectOptions::new()
			.filename(path)
			.create_if_missing(true)
			.foreign_keys(true);
		let pool = SqlitePoolOptions::new().max_connections(5).connect_with(options).await?;
		sqlx::raw_sql(SCHEMA).execute(&pool).await?;
		Ok(Self { pool })
	}

	pub async fn put_object(&self, object: &StoredObject) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT OR IGNORE INTO objects (digest, kind, payload, wire) VALUES (?1, ?2, ?3, ?4)")
			.bind(&object.digest)
			.bind(&object.kind)
			.bind(&object.payload)
			.bind(&object.wire)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn object(&self, digest: &[u8]) -> Result<Option<StoredObject>, sqlx::Error> {
		let row = sqlx::query("SELECT digest, kind, payload, wire FROM objects WHERE digest = ?1")
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
		let rows = sqlx::query("SELECT digest, kind, payload, wire FROM objects WHERE kind = ?1 LIMIT ?2")
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
		let row =
			sqlx::query("SELECT id, genesis_digest, head_seq, head_digest, profile_digest FROM projects WHERE id = ?1")
				.bind(id)
				.fetch_optional(&self.pool)
				.await?;
		Ok(row.map(|row| ProjectRow {
			id: row.get("id"),
			genesis_digest: row.get("genesis_digest"),
			head_seq: row.get("head_seq"),
			head_digest: row.get("head_digest"),
			profile_digest: row.get("profile_digest"),
		}))
	}

	pub async fn create_project(&self, id: &str, genesis_digest: &[u8]) -> Result<bool, sqlx::Error> {
		let result = sqlx::query("INSERT OR IGNORE INTO projects (id, genesis_digest) VALUES (?1, ?2)")
			.bind(id)
			.bind(genesis_digest)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn append_feed(&self, entry: &FeedRow) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		insert_feed_entry(&mut transaction, entry).await?;
		sqlx::query("UPDATE projects SET head_seq = ?1, head_digest = ?2 WHERE id = ?3")
			.bind(entry.seq)
			.bind(&entry.entry_digest)
			.bind(&entry.project_id)
			.execute(&mut *transaction)
			.await?;
		if entry.kind == "profile-updated" {
			sqlx::query("UPDATE projects SET profile_digest = ?1 WHERE id = ?2")
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
			 FROM feed_entries WHERE project_id = ?1 AND seq > ?2 ORDER BY seq ASC LIMIT ?3",
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
}

async fn insert_feed_entry(transaction: &mut Transaction<'_, sqlx::Sqlite>, entry: &FeedRow) -> Result<(), sqlx::Error> {
	sqlx::query(
		"INSERT INTO feed_entries (project_id, seq, previous, entry_digest, kind, object_digest, payload, wire)
		 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
	}
}
