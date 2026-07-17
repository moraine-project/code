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
	profile_digest BLOB,
	owner_kind TEXT,
	owner_id TEXT
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
CREATE TABLE IF NOT EXISTS orgs (
	id TEXT PRIMARY KEY,
	handle TEXT NOT NULL UNIQUE,
	display_name TEXT NOT NULL,
	created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS org_members (
	org_id TEXT NOT NULL,
	user_id TEXT NOT NULL,
	role TEXT NOT NULL,
	added_at INTEGER NOT NULL,
	PRIMARY KEY (org_id, user_id)
);
CREATE TABLE IF NOT EXISTS teams (
	id TEXT PRIMARY KEY,
	org_id TEXT NOT NULL,
	parent_team_id TEXT,
	display_name TEXT NOT NULL,
	created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS search_documents (
	project_id TEXT PRIMARY KEY,
	game_id TEXT NOT NULL,
	display_name TEXT NOT NULL,
	summary TEXT NOT NULL,
	updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS search_labels (
	project_id TEXT NOT NULL,
	label_kind TEXT NOT NULL,
	label_id TEXT NOT NULL,
	PRIMARY KEY (project_id, label_kind, label_id)
);
CREATE TABLE IF NOT EXISTS definition_subscriptions (
	home_url TEXT NOT NULL,
	id TEXT NOT NULL,
	kind TEXT NOT NULL,
	updated_at INTEGER NOT NULL,
	PRIMARY KEY (home_url, id)
);
CREATE TABLE IF NOT EXISTS definitions (
	id TEXT PRIMARY KEY,
	kind TEXT NOT NULL,
	genesis_digest BLOB NOT NULL,
	current_digest BLOB,
	created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS webhooks (
	id TEXT PRIMARY KEY,
	owner_id TEXT NOT NULL,
	url TEXT NOT NULL,
	event_kinds TEXT NOT NULL,
	created_at INTEGER NOT NULL,
	revoked_at INTEGER
);
CREATE TABLE IF NOT EXISTS webhook_deliveries (
	id TEXT PRIMARY KEY,
	webhook_id TEXT NOT NULL,
	event_id TEXT NOT NULL UNIQUE,
	url TEXT NOT NULL,
	body TEXT NOT NULL,
	attempt INTEGER NOT NULL DEFAULT 0,
	status TEXT NOT NULL,
	next_attempt_at INTEGER NOT NULL,
	created_at INTEGER NOT NULL,
	delivered_at INTEGER
);
CREATE TABLE IF NOT EXISTS follows (
	user_id TEXT NOT NULL,
	project_id TEXT NOT NULL,
	created_at INTEGER NOT NULL,
	PRIMARY KEY (user_id, project_id)
);
CREATE TABLE IF NOT EXISTS notifications (
	id TEXT PRIMARY KEY,
	user_id TEXT NOT NULL,
	project_id TEXT NOT NULL,
	event_kind TEXT NOT NULL,
	object_digest BLOB,
	feed_seq INTEGER,
	created_at INTEGER NOT NULL,
	read_at INTEGER
);
CREATE TABLE IF NOT EXISTS locations (
	artifact_digest BLOB NOT NULL,
	url TEXT NOT NULL,
	kind TEXT NOT NULL,
	operator_id TEXT,
	object_digest BLOB NOT NULL,
	PRIMARY KEY (artifact_digest, url)
);
CREATE TABLE IF NOT EXISTS mirrors (
	mirror_id TEXT PRIMARY KEY,
	public_key BLOB NOT NULL,
	added_by TEXT NOT NULL,
	added_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS mirror_commitments (
	artifact_digest BLOB NOT NULL,
	mirror_id TEXT NOT NULL,
	size INTEGER NOT NULL,
	accepted_at INTEGER NOT NULL,
	retention_until INTEGER,
	endpoint TEXT NOT NULL,
	object_digest BLOB NOT NULL,
	PRIMARY KEY (artifact_digest, mirror_id)
);
CREATE TABLE IF NOT EXISTS providers (
	provider_id TEXT PRIMARY KEY,
	public_key BLOB NOT NULL,
	added_by TEXT NOT NULL,
	added_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS advisories (
	digest BLOB PRIMARY KEY,
	provider_id TEXT NOT NULL,
	project_id TEXT NOT NULL,
	game_id TEXT NOT NULL,
	affected_digest BLOB,
	severity TEXT NOT NULL,
	category TEXT NOT NULL,
	block_promotion INTEGER NOT NULL,
	published_at INTEGER NOT NULL,
	retracted_at INTEGER
);
CREATE TABLE IF NOT EXISTS withdrawals (
	project_id TEXT NOT NULL,
	release_id TEXT NOT NULL,
	reason TEXT NOT NULL,
	note TEXT,
	declared_time INTEGER NOT NULL,
	PRIMARY KEY (project_id, release_id)
);
CREATE TABLE IF NOT EXISTS artifact_index (
	digest BLOB NOT NULL,
	project_id TEXT NOT NULL,
	release_digest BLOB NOT NULL,
	PRIMARY KEY (digest, release_digest)
);
CREATE TABLE IF NOT EXISTS subscriptions (
	home_url TEXT NOT NULL,
	project_id TEXT NOT NULL,
	cursor_seq INTEGER NOT NULL DEFAULT 0,
	status TEXT NOT NULL,
	updated_at INTEGER NOT NULL,
	PRIMARY KEY (home_url, project_id)
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
pub struct ArtifactMatchRow {
	pub project_id: String,
	pub release_digest: Vec<u8>,
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

#[derive(Debug, Clone)]
pub struct WithdrawalRow {
	pub reason: String,
	pub note: Option<String>,
	pub declared_time: i64,
}

pub struct MetadataStore {
	pub(crate) pool: SqlitePool,
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
		let row = sqlx::query(
			"SELECT id, genesis_digest, head_seq, head_digest, profile_digest, owner_kind, owner_id FROM projects WHERE id = ?1",
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
		let result = sqlx::query("UPDATE projects SET owner_kind = ?1, owner_id = ?2 WHERE id = ?3")
			.bind(owner_kind)
			.bind(owner_id)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected() == 1)
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

	pub async fn record_withdrawal(
		&self,
		project_id: &str,
		release_id: &str,
		reason: &str,
		note: Option<&str>,
		declared_time: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO withdrawals (project_id, release_id, reason, note, declared_time) VALUES (?1, ?2, ?3, ?4, ?5)
			 ON CONFLICT(project_id, release_id) DO UPDATE SET reason = ?3, note = ?4, declared_time = ?5",
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
			"SELECT release_id, reason, note, declared_time FROM withdrawals WHERE project_id = ?1 AND release_id = ?2",
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

	pub async fn index_artifact(&self, digest: &[u8], project_id: &str, release_digest: &[u8]) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT OR IGNORE INTO artifact_index (digest, project_id, release_digest) VALUES (?1, ?2, ?3)")
			.bind(digest)
			.bind(project_id)
			.bind(release_digest)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn artifacts_for_digest(&self, digest: &[u8]) -> Result<Vec<ArtifactMatchRow>, sqlx::Error> {
		let rows = sqlx::query("SELECT project_id, release_digest FROM artifact_index WHERE digest = ?1")
			.bind(digest)
			.fetch_all(&self.pool)
			.await?;
		Ok(rows
			.into_iter()
			.map(|row| ArtifactMatchRow {
				project_id: row.get("project_id"),
				release_digest: row.get("release_digest"),
			})
			.collect())
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

#[derive(Debug, Clone, Copy, Default)]
pub struct MetricsSnapshot {
	pub projects: i64,
	pub objects: i64,
	pub submissions: i64,
	pub submissions_pending: i64,
	pub review_decisions: i64,
	pub subscriptions: i64,
	pub deliveries_pending: i64,
	pub definitions: i64,
	pub advisories: i64,
	pub mirrors: i64,
	pub artifacts: i64,
	pub oldest_pending_submission: Option<i64>,
	pub oldest_pending_delivery: Option<i64>,
}

impl MetricsSnapshot {
	pub fn lines(&self) -> Vec<(&'static str, i64)> {
		vec![
			("moraine_projects_total", self.projects),
			("moraine_objects_total", self.objects),
			("moraine_submissions_total", self.submissions),
			("moraine_submissions_pending", self.submissions_pending),
			("moraine_review_decisions_total", self.review_decisions),
			("moraine_subscriptions_total", self.subscriptions),
			("moraine_deliveries_pending", self.deliveries_pending),
			("moraine_definitions_total", self.definitions),
			("moraine_advisories_total", self.advisories),
			("moraine_mirrors_total", self.mirrors),
			("moraine_artifacts_total", self.artifacts),
		]
	}
}

impl MetadataStore {
	pub async fn metrics_snapshot(&self) -> Result<MetricsSnapshot, sqlx::Error> {
		Ok(MetricsSnapshot {
			projects: self.table_count("projects").await?,
			objects: self.table_count("objects").await?,
			submissions: self.table_count("submissions").await?,
			submissions_pending: self
				.scalar_count("SELECT COUNT(*) FROM submissions WHERE state = 'submitted'")
				.await?,
			review_decisions: self.table_count("review_decisions").await?,
			subscriptions: self.table_count("subscriptions").await?,
			deliveries_pending: self
				.scalar_count("SELECT COUNT(*) FROM webhook_deliveries WHERE status = 'pending'")
				.await?,
			definitions: self.table_count("definitions").await?,
			advisories: self.table_count("advisories").await?,
			mirrors: self.table_count("mirrors").await?,
			artifacts: self.table_count("artifact_index").await?,
			oldest_pending_submission: self
				.scalar_opt("SELECT MIN(created_at) FROM submissions WHERE state = 'submitted'")
				.await?,
			oldest_pending_delivery: self
				.scalar_opt("SELECT MIN(next_attempt_at) FROM webhook_deliveries WHERE status = 'pending'")
				.await?,
		})
	}

	async fn table_count(&self, table: &str) -> Result<i64, sqlx::Error> {
		self.scalar_count(&format!("SELECT COUNT(*) FROM {table}")).await
	}

	async fn scalar_count(&self, query: &str) -> Result<i64, sqlx::Error> {
		sqlx::query_scalar::<_, i64>(query).fetch_one(&self.pool).await
	}

	async fn scalar_opt(&self, query: &str) -> Result<Option<i64>, sqlx::Error> {
		sqlx::query_scalar::<_, Option<i64>>(query).fetch_one(&self.pool).await
	}
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
}
