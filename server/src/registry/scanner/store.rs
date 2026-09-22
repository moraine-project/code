use serde_json;
use sqlx::Row;

use super::{Job, Provider, new_id};

impl crate::db::MetadataStore {
	pub async fn scanner_providers(&self) -> Result<Vec<Provider>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT provider_id, kind, command, args_json, public_key, enabled FROM scanner_providers ORDER BY provider_id",
		)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().filter_map(provider_from_row).collect())
	}

	pub async fn scanner_provider(&self, id: &str) -> Result<Option<Provider>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT provider_id, kind, command, args_json, public_key, enabled FROM scanner_providers WHERE provider_id = $1",
		)
		.bind(id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.and_then(provider_from_row))
	}

	pub async fn put_scanner_provider(&self, provider: &Provider, now: i64) -> Result<(), sqlx::Error> {
		let args = serde_json::to_string(&provider.args).map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
		sqlx::query("INSERT INTO scanner_providers (provider_id, kind, command, args_json, public_key, enabled, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT(provider_id) DO UPDATE SET kind = $2, command = $3, args_json = $4, public_key = $5, enabled = $6")
			.bind(&provider.id)
			.bind(&provider.kind)
			.bind(&provider.command)
			.bind(args)
			.bind(&provider.public_key)
			.bind(i64::from(provider.enabled))
			.bind(now)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn enqueue_scan(
		&self,
		provider_id: &str,
		digest: &[u8],
		requested_by: &str,
		now: i64,
	) -> Result<String, sqlx::Error> {
		let id = new_id();
		sqlx::query("INSERT INTO scan_jobs (id, provider_id, artifact_digest, status, requested_by, created_at) VALUES ($1, $2, $3, 'queued', $4, $5) ON CONFLICT(provider_id, artifact_digest) DO UPDATE SET status = CASE WHEN scan_jobs.status = 'running' THEN scan_jobs.status ELSE 'queued' END, requested_by = $4, created_at = $5")
			.bind(&id)
			.bind(provider_id)
			.bind(digest)
			.bind(requested_by)
			.bind(now)
			.execute(&self.pool)
			.await?;
		sqlx::query_scalar::<_, String>("SELECT id FROM scan_jobs WHERE provider_id = $1 AND artifact_digest = $2")
			.bind(provider_id)
			.bind(digest)
			.fetch_one(&self.pool)
			.await
	}

	pub async fn scan_jobs(&self, limit: i64) -> Result<Vec<Job>, sqlx::Error> {
		let rows = sqlx::query("SELECT id, provider_id, artifact_digest, status, requested_by, attempts, result_json, error FROM scan_jobs ORDER BY created_at DESC LIMIT $1")
			.bind(limit)
			.fetch_all(&self.pool)
			.await?;
		Ok(rows.into_iter().map(job_from_row).collect())
	}

	pub async fn claim_scan_job(&self, now: i64) -> Result<Option<Job>, sqlx::Error> {
		let row = sqlx::query("SELECT id FROM scan_jobs WHERE status = 'queued' ORDER BY created_at LIMIT 1")
			.fetch_optional(&self.pool)
			.await?;
		let Some(row) = row else { return Ok(None) };
		let id: String = row.get("id");
		let changed = sqlx::query(
			"UPDATE scan_jobs SET status = 'running', attempts = attempts + 1, started_at = $2 WHERE id = $1 AND status = 'queued'",
		)
		.bind(&id)
		.bind(now)
		.execute(&self.pool)
		.await?;
		if changed.rows_affected() == 0 {
			return Ok(None);
		}
		let row = sqlx::query("SELECT id, provider_id, artifact_digest, status, requested_by, attempts, result_json, error FROM scan_jobs WHERE id = $1")
			.bind(id)
			.fetch_one(&self.pool)
			.await?;
		Ok(Some(job_from_row(row)))
	}

	pub async fn finish_scan_job(
		&self,
		id: &str,
		status: &str,
		result: Option<&str>,
		error: Option<&str>,
		now: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE scan_jobs SET status = $2, result_json = $3, error = $4, finished_at = $5 WHERE id = $1")
			.bind(id)
			.bind(status)
			.bind(result)
			.bind(error)
			.bind(now)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn scanner_policies(&self) -> Result<Vec<(String, String, bool, bool)>, sqlx::Error> {
		let rows = sqlx::query("SELECT id, provider_id, enabled, auto_scan FROM scan_policies ORDER BY id")
			.fetch_all(&self.pool)
			.await?;
		Ok(rows
			.into_iter()
			.map(|row| {
				(
					row.get("id"),
					row.get("provider_id"),
					row.get::<i64, _>("enabled") != 0,
					row.get::<i64, _>("auto_scan") != 0,
				)
			})
			.collect())
	}

	pub async fn put_scanner_policy(
		&self,
		id: &str,
		provider_id: &str,
		enabled: bool,
		auto_scan: bool,
		now: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT INTO scan_policies (id, provider_id, enabled, auto_scan, created_at) VALUES ($1, $2, $3, $4, $5) ON CONFLICT(id) DO UPDATE SET provider_id = $2, enabled = $3, auto_scan = $4")
			.bind(id)
			.bind(provider_id)
			.bind(i64::from(enabled))
			.bind(i64::from(auto_scan))
			.bind(now)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn auto_scan_digests(&self) -> Result<Vec<(String, Vec<u8>)>, sqlx::Error> {
		let rows = sqlx::query("SELECT DISTINCT p.provider_id, a.digest FROM scan_policies p CROSS JOIN artifact_index a WHERE p.enabled = 1 AND p.auto_scan = 1")
			.fetch_all(&self.pool)
			.await?;
		Ok(rows
			.into_iter()
			.map(|row| (row.get("provider_id"), row.get("digest")))
			.collect())
	}
}

fn provider_from_row(row: sqlx::any::AnyRow) -> Option<Provider> {
	Some(Provider {
		id: row.get("provider_id"),
		kind: row.get("kind"),
		command: row.get("command"),
		args: serde_json::from_str(&row.get::<String, _>("args_json")).ok()?,
		public_key: row.get("public_key"),
		enabled: row.get::<i64, _>("enabled") != 0,
	})
}

fn job_from_row(row: sqlx::any::AnyRow) -> Job {
	Job {
		id: row.get("id"),
		provider_id: row.get("provider_id"),
		artifact_digest: row.get("artifact_digest"),
		status: row.get("status"),
		requested_by: row.get("requested_by"),
		attempts: row.get("attempts"),
		result_json: row.get("result_json"),
		error: row.get("error"),
	}
}
