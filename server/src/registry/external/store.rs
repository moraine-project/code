use serde_json::Value;
use sqlx::Row;

use crate::db::MetadataStore;

#[derive(Debug, Clone)]
pub(crate) struct ExternalProjectRow {
	pub provider: String,
	pub external_project_id: String,
	pub source_class: String,
	pub canonical_source_url: String,
	pub observed_profile: Value,
	pub observed_at: i64,
	pub last_synced_at: i64,
	pub source_state: String,
	pub bridge_id: String,
	pub bridge_version: String,
	pub linked_native_project_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExternalFileRow {
	pub external_file_id: String,
	pub source_url: String,
	pub digest: Option<Vec<u8>>,
	pub size: Option<i64>,
	pub metadata: Value,
	pub observed_at: i64,
	pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExternalClaimRow {
	pub id: String,
	pub provider: String,
	pub external_project_id: String,
	pub claimant_ref: String,
	pub challenge_ref: String,
	pub state: String,
	pub recorded_by: String,
	pub recorded_at: i64,
	pub expires_at: i64,
	pub verified_at: Option<i64>,
	pub reviewed_by: Option<String>,
}

impl MetadataStore {
	pub async fn upsert_external_project(
		&self,
		project: &ExternalProjectRow,
		files: &[ExternalFileRow],
	) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		sqlx::query(
			"INSERT INTO external_projects
			 (provider, external_project_id, source_class, canonical_source_url, observed_profile,
			  observed_at, last_synced_at, source_state, bridge_id, bridge_version, linked_native_project_id)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
			 ON CONFLICT(provider, external_project_id) DO UPDATE SET
			 source_class = $3, canonical_source_url = $4, observed_profile = $5, observed_at = $6,
			 last_synced_at = $7, source_state = $8, bridge_id = $9, bridge_version = $10,
			 linked_native_project_id = COALESCE($11, external_projects.linked_native_project_id)",
		)
		.bind(&project.provider)
		.bind(&project.external_project_id)
		.bind(&project.source_class)
		.bind(&project.canonical_source_url)
		.bind(project.observed_profile.to_string())
		.bind(project.observed_at)
		.bind(project.last_synced_at)
		.bind(&project.source_state)
		.bind(&project.bridge_id)
		.bind(&project.bridge_version)
		.bind(None::<String>)
		.execute(&mut *transaction)
		.await?;
		sqlx::query("DELETE FROM external_files WHERE provider = $1 AND external_project_id = $2")
			.bind(&project.provider)
			.bind(&project.external_project_id)
			.execute(&mut *transaction)
			.await?;
		for file in files {
			sqlx::query(
				"INSERT INTO external_files
				 (provider, external_file_id, external_project_id, source_url, digest, size, metadata_json, observed_at, deleted_at)
				 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
			)
			.bind(&project.provider)
			.bind(&file.external_file_id)
			.bind(&project.external_project_id)
			.bind(&file.source_url)
			.bind(&file.digest)
			.bind(file.size)
			.bind(file.metadata.to_string())
			.bind(file.observed_at)
			.bind(file.deleted_at)
			.execute(&mut *transaction)
			.await?;
		}
		transaction.commit().await
	}

	pub async fn external_project(
		&self,
		provider: &str,
		external_project_id: &str,
	) -> Result<Option<(ExternalProjectRow, Vec<ExternalFileRow>)>, sqlx::Error> {
		let project = sqlx::query(
			"SELECT provider, external_project_id, source_class, canonical_source_url, observed_profile,
			 observed_at, last_synced_at, source_state, bridge_id, bridge_version, linked_native_project_id
			 FROM external_projects WHERE provider = $1 AND external_project_id = $2",
		)
		.bind(provider)
		.bind(external_project_id)
		.fetch_optional(&self.pool)
		.await?;
		let Some(row) = project else { return Ok(None) };
		let project = ExternalProjectRow {
			provider: row.get("provider"),
			external_project_id: row.get("external_project_id"),
			source_class: row.get("source_class"),
			canonical_source_url: row.get("canonical_source_url"),
			observed_profile: parse_json(row.get("observed_profile"))?,
			observed_at: row.get("observed_at"),
			last_synced_at: row.get("last_synced_at"),
			source_state: row.get("source_state"),
			bridge_id: row.get("bridge_id"),
			bridge_version: row.get("bridge_version"),
			linked_native_project_id: row.get("linked_native_project_id"),
		};
		let rows = sqlx::query(
			"SELECT external_file_id, source_url, digest, size, metadata_json, observed_at, deleted_at
			 FROM external_files WHERE provider = $1 AND external_project_id = $2 ORDER BY external_file_id",
		)
		.bind(provider)
		.bind(external_project_id)
		.fetch_all(&self.pool)
		.await?;
		let files = rows
			.into_iter()
			.map(|row| {
				Ok(ExternalFileRow {
					external_file_id: row.get("external_file_id"),
					source_url: row.get("source_url"),
					digest: row.get("digest"),
					size: row.get("size"),
					metadata: parse_json(row.get("metadata_json"))?,
					observed_at: row.get("observed_at"),
					deleted_at: row.get("deleted_at"),
				})
			})
			.collect::<Result<Vec<_>, sqlx::Error>>()?;
		Ok(Some((project, files)))
	}

	pub async fn create_external_claim(&self, claim: &ExternalClaimRow) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO external_project_claims
			 (id, provider, external_project_id, claimant_ref, challenge_ref, state, recorded_by, recorded_at, expires_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
		)
		.bind(&claim.id)
		.bind(&claim.provider)
		.bind(&claim.external_project_id)
		.bind(&claim.claimant_ref)
		.bind(&claim.challenge_ref)
		.bind(&claim.state)
		.bind(&claim.recorded_by)
		.bind(claim.recorded_at)
		.bind(claim.expires_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn external_claims(&self, state: Option<&str>) -> Result<Vec<ExternalClaimRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, provider, external_project_id, claimant_ref, challenge_ref, state, recorded_by,
			 recorded_at, expires_at, verified_at, reviewed_by FROM external_project_claims
			 WHERE ($1 IS NULL OR state = $1) ORDER BY recorded_at DESC, id ASC",
		)
		.bind(state)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(claim_row).collect())
	}

	pub async fn review_external_claim(
		&self,
		id: &str,
		state: &str,
		reviewed_by: &str,
		verified_at: Option<i64>,
		now: i64,
	) -> Result<bool, sqlx::Error> {
		let result = sqlx::query(
			"UPDATE external_project_claims SET state = $2, reviewed_by = $3, verified_at = $4
			 WHERE id = $1 AND state = 'pending' AND expires_at > $5",
		)
		.bind(id)
		.bind(state)
		.bind(reviewed_by)
		.bind(verified_at)
		.bind(now)
		.execute(&self.pool)
		.await?;
		Ok(result.rows_affected() == 1)
	}
}

fn parse_json(value: String) -> Result<Value, sqlx::Error> {
	serde_json::from_str(&value).map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

fn claim_row(row: sqlx::any::AnyRow) -> ExternalClaimRow {
	ExternalClaimRow {
		id: row.get("id"),
		provider: row.get("provider"),
		external_project_id: row.get("external_project_id"),
		claimant_ref: row.get("claimant_ref"),
		challenge_ref: row.get("challenge_ref"),
		state: row.get("state"),
		recorded_by: row.get("recorded_by"),
		recorded_at: row.get("recorded_at"),
		expires_at: row.get("expires_at"),
		verified_at: row.get("verified_at"),
		reviewed_by: row.get("reviewed_by"),
	}
}
