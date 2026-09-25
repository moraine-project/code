use sqlx::Row;

use super::{AppendFeed, FeedRow, MetadataStore, ProjectRow, WithdrawalRow, insert_feed_entry};

fn is_unique_violation(error: &sqlx::Error) -> bool {
	matches!(
		error
			.as_database_error()
			.and_then(sqlx::error::DatabaseError::code)
			.as_deref(),
		Some("1555" | "2067" | "23505")
	)
}

impl MetadataStore {
	pub async fn project(&self, id: &str) -> Result<Option<ProjectRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT id, genesis_digest, head_seq, head_digest, profile_digest, owner_kind, owner_id, owner_key_id FROM projects WHERE id = $1",
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
			owner_key_id: row.get("owner_key_id"),
		}))
	}

	pub async fn set_project_owner(
		&self,
		id: &str,
		owner_kind: &str,
		owner_id: &str,
		owner_key_id: &[u8],
	) -> Result<bool, sqlx::Error> {
		let result = sqlx::query("UPDATE projects SET owner_kind = $1, owner_id = $2, owner_key_id = $3 WHERE id = $4")
			.bind(owner_kind)
			.bind(owner_id)
			.bind(owner_key_id)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn projects_owned_by(&self, owner_kind: &str, owner_id: &str) -> Result<Vec<String>, sqlx::Error> {
		let rows = sqlx::query("SELECT id FROM projects WHERE owner_kind = $1 AND owner_id = $2 ORDER BY id")
			.bind(owner_kind)
			.bind(owner_id)
			.fetch_all(&self.pool)
			.await?;
		Ok(rows.into_iter().map(|row| row.get("id")).collect())
	}

	pub async fn create_project(&self, id: &str, genesis_digest: &[u8]) -> Result<bool, sqlx::Error> {
		let result = sqlx::query("INSERT INTO projects (id, genesis_digest) VALUES ($1, $2) ON CONFLICT DO NOTHING")
			.bind(id)
			.bind(genesis_digest)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn append_feed(&self, entry: &FeedRow) -> Result<AppendFeed, sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		if let Err(error) = insert_feed_entry(&mut transaction, entry).await {
			if is_unique_violation(&error) {
				transaction.rollback().await?;
				return Ok(AppendFeed::HeadMoved);
			}
			return Err(error);
		}
		let moved = sqlx::query("UPDATE projects SET head_seq = $1, head_digest = $2 WHERE id = $3 AND head_seq = $4")
			.bind(entry.seq)
			.bind(&entry.entry_digest)
			.bind(&entry.project_id)
			.bind(entry.seq - 1)
			.execute(&mut *transaction)
			.await?
			.rows_affected()
			== 1;
		if !moved {
			transaction.rollback().await?;
			return Ok(AppendFeed::HeadMoved);
		}
		if entry.kind == "profile-updated" {
			sqlx::query("UPDATE projects SET profile_digest = $1 WHERE id = $2")
				.bind(&entry.object_digest)
				.bind(&entry.project_id)
				.execute(&mut *transaction)
				.await?;
		}
		transaction.commit().await?;
		Ok(AppendFeed::Appended)
	}

	pub async fn feed_at(&self, project_id: &str, seq: i64) -> Result<Option<FeedRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT project_id, seq, previous, entry_digest, kind, object_digest, payload, wire
			 FROM feed_entries WHERE project_id = $1 AND seq = $2",
		)
		.bind(project_id)
		.bind(seq)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| FeedRow {
			project_id: row.get("project_id"),
			seq: row.get("seq"),
			previous: row.get("previous"),
			entry_digest: row.get("entry_digest"),
			kind: row.get("kind"),
			object_digest: row.get("object_digest"),
			payload: row.get("payload"),
			wire: row.get("wire"),
		}))
	}

	pub async fn feed_after(
		&self,
		project_id: &str,
		after: i64,
		through: i64,
		limit: i64,
	) -> Result<Vec<FeedRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT project_id, seq, previous, entry_digest, kind, object_digest, payload, wire
			 FROM feed_entries WHERE project_id = $1 AND seq > $2 AND seq <= $3 ORDER BY seq ASC LIMIT $4",
		)
		.bind(project_id)
		.bind(after)
		.bind(through)
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
