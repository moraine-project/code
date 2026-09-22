use sqlx::Row;

use crate::db::MetadataStore;

#[derive(Debug, Clone)]
pub(crate) struct SubmissionRow {
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
pub(crate) struct ReviewDecisionRow {
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

impl MetadataStore {
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
