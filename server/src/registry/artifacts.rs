use sqlx::Row;

use crate::db::MetadataStore;
use crate::db::sql::SqlBuilder;

#[derive(Debug, Clone)]
pub struct ArtifactMatchRow {
	pub project_id: String,
	pub release_digest: Vec<u8>,
}

impl MetadataStore {
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

	pub async fn record_upload(&self, digest: &[u8], user_id: &str, size: i64, created_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO blob_uploads (artifact_digest, user_id, size, created_at) VALUES ($1, $2, $3, $4)
			 ON CONFLICT (artifact_digest) DO NOTHING",
		)
		.bind(digest)
		.bind(user_id)
		.bind(size)
		.bind(created_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn upload_bytes_for(&self, user_id: &str) -> Result<i64, sqlx::Error> {
		let total = sqlx::query_scalar::<_, i64>(
			"SELECT CAST(COALESCE(SUM(size), 0) AS BIGINT) FROM blob_uploads WHERE user_id = $1",
		)
		.bind(user_id)
		.fetch_one(&self.pool)
		.await?;
		Ok(total)
	}

	pub async fn forget_upload(&self, digest: &[u8]) -> Result<(), sqlx::Error> {
		sqlx::query("DELETE FROM blob_uploads WHERE artifact_digest = $1")
			.bind(digest)
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
}
