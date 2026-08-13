pub mod sql;

use std::path::Path;

use sqlx::any::AnyPoolOptions;
use sqlx::{AnyPool, Row, Transaction};

use crate::db::sql::SqlBuilder;

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
	pub assigned_to: Option<String>,
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

	pub async fn index_artifact(&self, digest: &[u8], project_id: &str, release_digest: &[u8]) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO artifact_index (digest, project_id, release_digest) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
		)
		.bind(digest)
		.bind(project_id)
		.bind(release_digest)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn blob_is_referenced(&self, digest: &[u8]) -> Result<bool, sqlx::Error> {
		let referenced = sqlx::query_scalar::<_, i64>(
			"SELECT COUNT(*) FROM (
				SELECT digest FROM artifact_index WHERE digest = $1
				UNION SELECT artifact_digest FROM locations WHERE artifact_digest = $1
				UNION SELECT artifact_digest FROM mirror_commitments WHERE artifact_digest = $1)",
		)
		.bind(digest)
		.fetch_one(&self.pool)
		.await?;
		Ok(referenced > 0)
	}

	pub async fn record_download(&self, digest: &[u8], day: i64) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO download_counts (project_id, day, count)
			 SELECT DISTINCT project_id, $2, 1 FROM artifact_index WHERE digest = $1
			 ON CONFLICT(project_id, day) DO UPDATE SET count = download_counts.count + 1",
		)
		.bind(digest)
		.bind(day)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn popularities(
		&self,
		project_ids: &[String],
		since_day: i64,
	) -> Result<std::collections::HashMap<String, i64>, sqlx::Error> {
		let mut totals = std::collections::HashMap::new();
		if project_ids.is_empty() {
			return Ok(totals);
		}
		let mut downloads =
			SqlBuilder::new("SELECT project_id, CAST(SUM(count) AS BIGINT) AS total FROM download_counts WHERE day >= ");
		downloads.push_bind(since_day).push(" AND project_id IN (");
		{
			let mut separated = downloads.separated(", ");
			for project_id in project_ids {
				separated.push_bind(project_id);
			}
		}
		downloads.push(") GROUP BY project_id");
		for row in downloads.into_query().fetch_all(&self.pool).await? {
			let project_id: String = row.get("project_id");
			let total: i64 = row.get("total");
			*totals.entry(project_id).or_insert(0) += total;
		}
		let mut follows = SqlBuilder::new("SELECT project_id, COUNT(*) AS total FROM follows WHERE created_at >= ");
		follows.push_bind(since_day * 86_400).push(" AND project_id IN (");
		{
			let mut separated = follows.separated(", ");
			for project_id in project_ids {
				separated.push_bind(project_id);
			}
		}
		follows.push(") GROUP BY project_id");
		for row in follows.into_query().fetch_all(&self.pool).await? {
			let project_id: String = row.get("project_id");
			let total: i64 = row.get("total");
			*totals.entry(project_id).or_insert(0) += total;
		}
		Ok(totals)
	}

	pub async fn artifacts_for_digest(&self, digest: &[u8]) -> Result<Vec<ArtifactMatchRow>, sqlx::Error> {
		let rows = sqlx::query("SELECT project_id, release_digest FROM artifact_index WHERE digest = $1")
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
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)",
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
			"SELECT id, project_id, object_digest, entry_digest, entry_wire, state, assigned_to, submitted_by, created_at, updated_at
			 FROM submissions WHERE id = $1",
		)
		.bind(id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(submission_row))
	}

	pub async fn open_submissions(
		&self,
		limit: i64,
		cursor: Option<(i64, &str)>,
	) -> Result<Vec<SubmissionRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, project_id, object_digest, entry_digest, entry_wire, state, assigned_to, submitted_by, created_at, updated_at
			 FROM submissions WHERE state IN ('submitted', 'under_review')
			 AND (created_at > $1 OR (created_at = $1 AND ($3 = 0 OR id > $2)))
			 ORDER BY created_at ASC, id ASC LIMIT $4",
		)
		.bind(cursor.map(|(created, _)| created).unwrap_or(i64::MIN))
		.bind(cursor.map(|(_, id)| id).unwrap_or(""))
		.bind(i64::from(cursor.is_some()))
		.bind(limit)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(submission_row).collect())
	}

	pub async fn submissions_by_submitter(
		&self,
		user_id: &str,
		limit: i64,
		cursor: Option<(i64, &str)>,
	) -> Result<Vec<SubmissionRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, project_id, object_digest, entry_digest, entry_wire, state, assigned_to, submitted_by, created_at, updated_at
			 FROM submissions WHERE submitted_by = $1
			 AND (created_at < $2 OR (created_at = $2 AND ($4 = 0 OR id < $3)))
			 ORDER BY created_at DESC, id DESC LIMIT $5",
		)
		.bind(user_id)
		.bind(cursor.map(|(created, _)| created).unwrap_or(i64::MAX))
		.bind(cursor.map(|(_, id)| id).unwrap_or(""))
		.bind(i64::from(cursor.is_some()))
		.bind(limit)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(submission_row).collect())
	}

	pub async fn assign_submission(&self, id: &str, reviewer_id: &str, updated_at: i64) -> Result<bool, sqlx::Error> {
		let result = sqlx::query(
			"UPDATE submissions SET state = 'under_review', assigned_to = $1, updated_at = $2 WHERE id = $3 AND state = 'submitted'",
		)
		.bind(reviewer_id)
		.bind(updated_at)
		.bind(id)
		.execute(&self.pool)
		.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn set_submission_state(&self, id: &str, state: &str, updated_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE submissions SET state = $1, updated_at = $2 WHERE id = $3")
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
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
		)
		.bind(&decision.id)
		.bind(&decision.submission_id)
		.bind(&decision.object_digest)
		.bind(&decision.reviewer_id)
		.bind(&decision.decision)
		.bind(&decision.reason_code)
		.bind(i64::from(decision.reason_taxonomy_version))
		.bind(decision.decided_at)
		.bind(&decision.appeal_route)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn decisions_for(&self, submission_id: &str) -> Result<Vec<ReviewDecisionRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, submission_id, object_digest, reviewer_id, decision, reason_code, reason_taxonomy_version, decided_at, appeal_route
			 FROM review_decisions WHERE submission_id = $1 ORDER BY decided_at ASC",
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

fn submission_row(row: sqlx::any::AnyRow) -> SubmissionRow {
	SubmissionRow {
		id: row.get("id"),
		project_id: row.get("project_id"),
		object_digest: row.get("object_digest"),
		entry_digest: row.get("entry_digest"),
		entry_wire: row.get("entry_wire"),
		state: row.get("state"),
		assigned_to: row.get("assigned_to"),
		submitted_by: row.get("submitted_by"),
		created_at: row.get("created_at"),
		updated_at: row.get("updated_at"),
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
	pub subscription_lag: i64,
	pub subscription_resets: i64,
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
			("moraine_subscription_lag_entries", self.subscription_lag),
			("moraine_subscription_resets", self.subscription_resets),
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
			subscription_lag: self
				.scalar_opt("SELECT MAX(remote_head_seq - cursor_seq) FROM subscriptions")
				.await?
				.unwrap_or(0),
			subscription_resets: self
				.scalar_opt("SELECT CAST(SUM(reset_count) AS BIGINT) FROM subscriptions")
				.await?
				.unwrap_or(0),
			oldest_pending_submission: self
				.scalar_opt("SELECT MIN(created_at) FROM submissions WHERE state = 'submitted'")
				.await?,
			oldest_pending_delivery: self
				.scalar_opt("SELECT MIN(next_attempt_at) FROM webhook_deliveries WHERE status = 'pending'")
				.await?,
		})
	}

	pub async fn referenced_blob_digests(&self) -> Result<Vec<Vec<u8>>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT digest FROM artifact_index
			 UNION SELECT artifact_digest FROM locations
			 UNION SELECT artifact_digest FROM mirror_commitments",
		)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(|row| row.get::<Vec<u8>, _>(0)).collect())
	}

	async fn table_count(&self, table: &str) -> Result<i64, sqlx::Error> {
		self.scalar_count(&format!("SELECT COUNT(*) FROM {table}")).await
	}

	async fn scalar_count(&self, query: &str) -> Result<i64, sqlx::Error> {
		sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(query))
			.fetch_one(&self.pool)
			.await
	}

	async fn scalar_opt(&self, query: &str) -> Result<Option<i64>, sqlx::Error> {
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
