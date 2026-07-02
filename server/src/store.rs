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
CREATE TABLE IF NOT EXISTS users (
	id TEXT PRIMARY KEY,
	email TEXT NOT NULL UNIQUE,
	created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS user_credentials (
	user_id TEXT PRIMARY KEY,
	secret_hash TEXT NOT NULL,
	updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS user_sessions (
	id TEXT PRIMARY KEY,
	user_id TEXT NOT NULL,
	token_hash BLOB NOT NULL UNIQUE,
	created_at INTEGER NOT NULL,
	last_used_at INTEGER NOT NULL,
	idle_expires_at INTEGER NOT NULL,
	absolute_expires_at INTEGER NOT NULL,
	revoked_at INTEGER
);
CREATE TABLE IF NOT EXISTS api_keys (
	id TEXT PRIMARY KEY,
	user_id TEXT NOT NULL,
	name TEXT NOT NULL,
	prefix TEXT NOT NULL,
	secret_hash BLOB NOT NULL UNIQUE,
	scopes TEXT NOT NULL,
	created_at INTEGER NOT NULL,
	expires_at INTEGER,
	revoked_at INTEGER,
	last_used_at INTEGER
);
CREATE TABLE IF NOT EXISTS submissions (
	id TEXT PRIMARY KEY,
	project_id TEXT NOT NULL,
	object_digest BLOB NOT NULL,
	entry_digest BLOB NOT NULL,
	entry_wire BLOB NOT NULL,
	state TEXT NOT NULL,
	submitted_by TEXT NOT NULL,
	created_at INTEGER NOT NULL,
	updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS review_decisions (
	id TEXT PRIMARY KEY,
	submission_id TEXT NOT NULL,
	object_digest BLOB NOT NULL,
	reviewer_id TEXT NOT NULL,
	decision TEXT NOT NULL,
	reason_code TEXT,
	reason_taxonomy_version INTEGER NOT NULL,
	decided_at INTEGER NOT NULL,
	appeal_route TEXT
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

#[derive(Debug, Clone)]
pub struct UserRow {
	pub id: String,
	pub email: String,
	pub password_hash: String,
}

#[derive(Debug, Clone)]
pub struct SessionRow {
	pub id: String,
	pub user_id: String,
	pub token_hash: Vec<u8>,
	pub created_at: i64,
	pub idle_expires_at: i64,
	pub absolute_expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct ApiKeyRow {
	pub id: String,
	pub user_id: String,
	pub name: String,
	pub prefix: String,
	pub secret_hash: Vec<u8>,
	pub scopes: String,
	pub created_at: i64,
	pub expires_at: Option<i64>,
	pub last_used_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct SubmissionRow {
	pub id: String,
	pub project_id: String,
	pub object_digest: Vec<u8>,
	pub entry_digest: Vec<u8>,
	pub entry_wire: Vec<u8>,
	pub state: String,
	pub submitted_by: String,
	pub created_at: i64,
	pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct ReviewDecisionRow {
	pub id: String,
	pub submission_id: String,
	pub object_digest: Vec<u8>,
	pub reviewer_id: String,
	pub decision: String,
	pub reason_code: Option<String>,
	pub reason_taxonomy_version: u32,
	pub decided_at: i64,
	pub appeal_route: Option<String>,
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

	pub async fn create_user(&self, id: &str, email: &str, password_hash: &str, created_at: i64) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		sqlx::query("INSERT INTO users (id, email, created_at) VALUES (?1, ?2, ?3)")
			.bind(id)
			.bind(email)
			.bind(created_at)
			.execute(&mut *transaction)
			.await?;
		sqlx::query("INSERT INTO user_credentials (user_id, secret_hash, updated_at) VALUES (?1, ?2, ?3)")
			.bind(id)
			.bind(password_hash)
			.bind(created_at)
			.execute(&mut *transaction)
			.await?;
		transaction.commit().await?;
		Ok(())
	}

	pub async fn user_by_email(&self, email: &str) -> Result<Option<UserRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT u.id, u.email, c.secret_hash
			 FROM users u JOIN user_credentials c ON c.user_id = u.id WHERE u.email = ?1",
		)
		.bind(email)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| UserRow {
			id: row.get("id"),
			email: row.get("email"),
			password_hash: row.get("secret_hash"),
		}))
	}

	pub async fn user_by_id(&self, id: &str) -> Result<Option<UserRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT u.id, u.email, c.secret_hash
			 FROM users u JOIN user_credentials c ON c.user_id = u.id WHERE u.id = ?1",
		)
		.bind(id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| UserRow {
			id: row.get("id"),
			email: row.get("email"),
			password_hash: row.get("secret_hash"),
		}))
	}

	pub async fn create_session(&self, session: &SessionRow) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO user_sessions (id, user_id, token_hash, created_at, last_used_at, idle_expires_at, absolute_expires_at)
			 VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6)",
		)
		.bind(&session.id)
		.bind(&session.user_id)
		.bind(&session.token_hash)
		.bind(session.created_at)
		.bind(session.idle_expires_at)
		.bind(session.absolute_expires_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn session_by_token(&self, token_hash: &[u8], now: i64) -> Result<Option<SessionRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT id, user_id, token_hash, created_at, idle_expires_at, absolute_expires_at FROM user_sessions
			 WHERE token_hash = ?1 AND revoked_at IS NULL AND idle_expires_at > ?2 AND absolute_expires_at > ?2",
		)
		.bind(token_hash)
		.bind(now)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| SessionRow {
			id: row.get("id"),
			user_id: row.get("user_id"),
			token_hash: row.get("token_hash"),
			created_at: row.get("created_at"),
			idle_expires_at: row.get("idle_expires_at"),
			absolute_expires_at: row.get("absolute_expires_at"),
		}))
	}

	pub async fn touch_session(&self, id: &str, last_used_at: i64, idle_expires_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE user_sessions SET last_used_at = ?1, idle_expires_at = ?2 WHERE id = ?3")
			.bind(last_used_at)
			.bind(idle_expires_at)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn revoke_session(&self, id: &str, revoked_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE user_sessions SET revoked_at = ?1 WHERE id = ?2")
			.bind(revoked_at)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn create_api_key(&self, key: &ApiKeyRow) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO api_keys (id, user_id, name, prefix, secret_hash, scopes, created_at, expires_at)
			 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
		)
		.bind(&key.id)
		.bind(&key.user_id)
		.bind(&key.name)
		.bind(&key.prefix)
		.bind(&key.secret_hash)
		.bind(&key.scopes)
		.bind(key.created_at)
		.bind(key.expires_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn api_keys_for_user(&self, user_id: &str) -> Result<Vec<ApiKeyRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, user_id, name, prefix, scopes, created_at, expires_at, last_used_at
			 FROM api_keys WHERE user_id = ?1 AND revoked_at IS NULL ORDER BY created_at",
		)
		.bind(user_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| ApiKeyRow {
				id: row.get("id"),
				user_id: row.get("user_id"),
				name: row.get("name"),
				prefix: row.get("prefix"),
				secret_hash: Vec::new(),
				scopes: row.get("scopes"),
				created_at: row.get("created_at"),
				expires_at: row.get("expires_at"),
				last_used_at: row.get("last_used_at"),
			})
			.collect())
	}

	pub async fn api_key_by_token(&self, token_hash: &[u8], now: i64) -> Result<Option<ApiKeyRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT id, user_id, name, prefix, secret_hash, scopes, created_at, expires_at, last_used_at FROM api_keys
			 WHERE secret_hash = ?1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?2)",
		)
		.bind(token_hash)
		.bind(now)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| ApiKeyRow {
			id: row.get("id"),
			user_id: row.get("user_id"),
			name: row.get("name"),
			prefix: row.get("prefix"),
			secret_hash: row.get("secret_hash"),
			scopes: row.get("scopes"),
			created_at: row.get("created_at"),
			expires_at: row.get("expires_at"),
			last_used_at: row.get("last_used_at"),
		}))
	}

	pub async fn touch_api_key(&self, id: &str, last_used_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE api_keys SET last_used_at = ?1 WHERE id = ?2")
			.bind(last_used_at)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn revoke_api_key(&self, user_id: &str, key_id: &str, revoked_at: i64) -> Result<bool, sqlx::Error> {
		let result =
			sqlx::query("UPDATE api_keys SET revoked_at = ?1 WHERE id = ?2 AND user_id = ?3 AND revoked_at IS NULL")
				.bind(revoked_at)
				.bind(key_id)
				.bind(user_id)
				.execute(&self.pool)
				.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn create_submission(&self, submission: &SubmissionRow) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO submissions (id, project_id, object_digest, entry_digest, entry_wire, state, submitted_by, created_at, updated_at)
			 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
		)
		.bind(&submission.id)
		.bind(&submission.project_id)
		.bind(&submission.object_digest)
		.bind(&submission.entry_digest)
		.bind(&submission.entry_wire)
		.bind(&submission.state)
		.bind(&submission.submitted_by)
		.bind(submission.created_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn submission(&self, id: &str) -> Result<Option<SubmissionRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT id, project_id, object_digest, entry_digest, entry_wire, state, submitted_by, created_at, updated_at
			 FROM submissions WHERE id = ?1",
		)
		.bind(id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(submission_from_row))
	}

	pub async fn submissions_in_state(&self, state: &str, limit: i64) -> Result<Vec<SubmissionRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, project_id, object_digest, entry_digest, entry_wire, state, submitted_by, created_at, updated_at
			 FROM submissions WHERE state = ?1 ORDER BY created_at ASC LIMIT ?2",
		)
		.bind(state)
		.bind(limit)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(submission_from_row).collect())
	}

	pub async fn set_submission_state(&self, id: &str, state: &str, updated_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE submissions SET state = ?1, updated_at = ?2 WHERE id = ?3")
			.bind(state)
			.bind(updated_at)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn insert_decision(&self, decision: &ReviewDecisionRow) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO review_decisions (id, submission_id, object_digest, reviewer_id, decision, reason_code, reason_taxonomy_version, decided_at, appeal_route)
			 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
		)
		.bind(&decision.id)
		.bind(&decision.submission_id)
		.bind(&decision.object_digest)
		.bind(&decision.reviewer_id)
		.bind(&decision.decision)
		.bind(&decision.reason_code)
		.bind(decision.reason_taxonomy_version)
		.bind(decision.decided_at)
		.bind(&decision.appeal_route)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn decisions_for(&self, submission_id: &str) -> Result<Vec<ReviewDecisionRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, submission_id, object_digest, reviewer_id, decision, reason_code, reason_taxonomy_version, decided_at, appeal_route
			 FROM review_decisions WHERE submission_id = ?1 ORDER BY decided_at ASC",
		)
		.bind(submission_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| ReviewDecisionRow {
				id: row.get("id"),
				submission_id: row.get("submission_id"),
				object_digest: row.get("object_digest"),
				reviewer_id: row.get("reviewer_id"),
				decision: row.get("decision"),
				reason_code: row.get("reason_code"),
				reason_taxonomy_version: row.get::<i64, _>("reason_taxonomy_version") as u32,
				decided_at: row.get("decided_at"),
				appeal_route: row.get("appeal_route"),
			})
			.collect())
	}
}

fn submission_from_row(row: sqlx::sqlite::SqliteRow) -> SubmissionRow {
	SubmissionRow {
		id: row.get("id"),
		project_id: row.get("project_id"),
		object_digest: row.get("object_digest"),
		entry_digest: row.get("entry_digest"),
		entry_wire: row.get("entry_wire"),
		state: row.get("state"),
		submitted_by: row.get("submitted_by"),
		created_at: row.get("created_at"),
		updated_at: row.get("updated_at"),
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
