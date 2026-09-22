use sqlx::Row;

use crate::db::MetadataStore;

#[derive(Debug, Clone)]
pub struct LocationRow {
	pub url: String,
	pub kind: String,
	pub operator_id: Option<String>,
	pub object_digest: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct DueCommitmentRow {
	pub artifact_digest: Vec<u8>,
	pub mirror_id: String,
	pub endpoint: String,
	pub size: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct ConfirmationRow {
	pub checked_at: i64,
	pub reachable: bool,
}

#[derive(Debug, Clone)]
pub struct CommitmentRow {
	pub mirror_id: String,
	pub size: i64,
	pub accepted_at: i64,
	pub retention_until: Option<i64>,
	pub endpoint: String,
	pub object_digest: Vec<u8>,
}

impl MetadataStore {
	pub async fn pin_mirror(
		&self,
		mirror_id: &str,
		public_key: &[u8],
		added_by: &str,
		added_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO mirrors (mirror_id, public_key, added_by, added_at) VALUES ($1, $2, $3, $4)
			 ON CONFLICT(mirror_id) DO UPDATE SET public_key = $2, added_by = $3, added_at = $4",
		)
		.bind(mirror_id)
		.bind(public_key)
		.bind(added_by)
		.bind(added_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn mirror_key(&self, mirror_id: &str) -> Result<Option<Vec<u8>>, sqlx::Error> {
		let row = sqlx::query("SELECT public_key FROM mirrors WHERE mirror_id = $1")
			.bind(mirror_id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.map(|row| row.get("public_key")))
	}

	pub async fn index_location(
		&self,
		artifact_digest: &[u8],
		url: &str,
		kind: &str,
		operator_id: Option<&str>,
		object_digest: &[u8],
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO locations (artifact_digest, url, kind, operator_id, object_digest) VALUES ($1, $2, $3, $4, $5)
			 ON CONFLICT (artifact_digest, url) DO UPDATE SET kind = $3, operator_id = $4, object_digest = $5",
		)
		.bind(artifact_digest)
		.bind(url)
		.bind(kind)
		.bind(operator_id)
		.bind(object_digest)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn locations_for(&self, artifact_digest: &[u8]) -> Result<Vec<LocationRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT url, kind, operator_id, object_digest FROM locations WHERE artifact_digest = $1 ORDER BY url",
		)
		.bind(artifact_digest)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| LocationRow {
				url: row.get("url"),
				kind: row.get("kind"),
				operator_id: row.get("operator_id"),
				object_digest: row.get("object_digest"),
			})
			.collect())
	}

	pub async fn insert_commitment(
		&self,
		commitment: &CommitmentRow,
		artifact_digest: &[u8],
		object_digest: &[u8],
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO mirror_commitments (artifact_digest, mirror_id, size, accepted_at, retention_until, endpoint, object_digest)
			 VALUES ($1, $2, $3, $4, $5, $6, $7)
			 ON CONFLICT (artifact_digest, mirror_id) DO UPDATE SET size = $3, accepted_at = $4,
			 retention_until = $5, endpoint = $6, object_digest = $7",
		)
		.bind(artifact_digest)
		.bind(&commitment.mirror_id)
		.bind(commitment.size)
		.bind(commitment.accepted_at)
		.bind(commitment.retention_until)
		.bind(&commitment.endpoint)
		.bind(object_digest)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn record_confirmation(
		&self,
		artifact_digest: &[u8],
		mirror_id: &str,
		checked_at: i64,
		reachable: bool,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO mirror_confirmations (artifact_digest, mirror_id, checked_at, reachable)
			 VALUES ($1, $2, $3, $4)
			 ON CONFLICT (artifact_digest, mirror_id) DO UPDATE SET checked_at = $3, reachable = $4",
		)
		.bind(artifact_digest)
		.bind(mirror_id)
		.bind(checked_at)
		.bind(reachable)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn confirmation_for(
		&self,
		artifact_digest: &[u8],
		mirror_id: &str,
	) -> Result<Option<ConfirmationRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT checked_at, reachable FROM mirror_confirmations WHERE artifact_digest = $1 AND mirror_id = $2",
		)
		.bind(artifact_digest)
		.bind(mirror_id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| ConfirmationRow {
			checked_at: row.get("checked_at"),
			reachable: row.get::<i64, _>("reachable") != 0,
		}))
	}

	pub async fn commitments_due(
		&self,
		before: i64,
		limit: i64,
		max_bytes: u64,
	) -> Result<Vec<DueCommitmentRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT c.artifact_digest, c.mirror_id, c.endpoint, c.size
			 FROM mirror_commitments c
			 LEFT JOIN mirror_confirmations f
			   ON f.artifact_digest = c.artifact_digest AND f.mirror_id = c.mirror_id
			 WHERE (f.checked_at IS NULL OR f.checked_at < $1) AND c.size <= $3
			 ORDER BY COALESCE(f.checked_at, 0) ASC, c.artifact_digest ASC
			 LIMIT $2",
		)
		.bind(before)
		.bind(limit)
		.bind(max_bytes as i64)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| DueCommitmentRow {
				artifact_digest: row.get("artifact_digest"),
				mirror_id: row.get("mirror_id"),
				endpoint: row.get("endpoint"),
				size: row.get("size"),
			})
			.collect())
	}

	pub async fn commitments_for(&self, artifact_digest: &[u8]) -> Result<Vec<CommitmentRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT mirror_id, size, accepted_at, retention_until, endpoint, object_digest FROM mirror_commitments WHERE artifact_digest = $1 ORDER BY accepted_at DESC",
		)
		.bind(artifact_digest)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| CommitmentRow {
				mirror_id: row.get("mirror_id"),
				size: row.get("size"),
				accepted_at: row.get("accepted_at"),
				retention_until: row.get("retention_until"),
				endpoint: row.get("endpoint"),
				object_digest: row.get("object_digest"),
			})
			.collect())
	}
}
