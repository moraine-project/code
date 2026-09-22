use sqlx::Row;

use crate::db::MetadataStore;

#[derive(Debug, Clone)]
pub struct WebhookRow {
	pub id: String,
	pub url: String,
	pub event_kinds: String,
	pub created_at: i64,
}

pub(super) struct DeliveryRow {
	pub(super) id: String,
	pub(super) url: String,
	pub(super) body: String,
	pub(super) attempt: i64,
}

impl MetadataStore {
	pub async fn create_webhook(
		&self,
		id: &str,
		owner_id: &str,
		url: &str,
		event_kinds: &str,
		created_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT INTO webhooks (id, owner_id, url, event_kinds, created_at) VALUES ($1, $2, $3, $4, $5)")
			.bind(id)
			.bind(owner_id)
			.bind(url)
			.bind(event_kinds)
			.bind(created_at)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn webhooks_for_owner(&self, owner_id: &str) -> Result<Vec<WebhookRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, url, event_kinds, created_at FROM webhooks WHERE owner_id = $1 AND revoked_at IS NULL ORDER BY created_at",
		)
		.bind(owner_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(webhook_from_row).collect())
	}

	pub async fn active_webhooks(&self) -> Result<Vec<WebhookRow>, sqlx::Error> {
		let rows = sqlx::query("SELECT id, url, event_kinds, created_at FROM webhooks WHERE revoked_at IS NULL")
			.fetch_all(&self.pool)
			.await?;
		Ok(rows.into_iter().map(webhook_from_row).collect())
	}

	pub async fn revoke_webhook(&self, owner_id: &str, id: &str, revoked_at: i64) -> Result<bool, sqlx::Error> {
		let result =
			sqlx::query("UPDATE webhooks SET revoked_at = $1 WHERE id = $2 AND owner_id = $3 AND revoked_at IS NULL")
				.bind(revoked_at)
				.bind(id)
				.bind(owner_id)
				.execute(&self.pool)
				.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn enqueue_delivery(
		&self,
		id: &str,
		webhook_id: &str,
		event_id: &str,
		url: &str,
		body: &str,
		next_attempt_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO webhook_deliveries (id, webhook_id, event_id, url, body, status, next_attempt_at, created_at)
			 VALUES ($1, $2, $3, $4, $5, 'pending', $6, $6) ON CONFLICT DO NOTHING",
		)
		.bind(id)
		.bind(webhook_id)
		.bind(event_id)
		.bind(url)
		.bind(body)
		.bind(next_attempt_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub(super) async fn due_deliveries(&self, now: i64, limit: i64) -> Result<Vec<DeliveryRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, url, body, attempt FROM webhook_deliveries WHERE status = 'pending' AND next_attempt_at <= $1 ORDER BY next_attempt_at LIMIT $2",
		)
		.bind(now)
		.bind(limit)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| DeliveryRow {
				id: row.get("id"),
				url: row.get("url"),
				body: row.get("body"),
				attempt: row.get("attempt"),
			})
			.collect())
	}

	pub async fn prune_deliveries(&self, before: i64) -> Result<u64, sqlx::Error> {
		let result = sqlx::query("DELETE FROM webhook_deliveries WHERE created_at < $1")
			.bind(before)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected())
	}

	pub(super) async fn finish_delivery(
		&self,
		id: &str,
		attempt: i64,
		status: &str,
		next_attempt_at: i64,
		delivered_at: Option<i64>,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"UPDATE webhook_deliveries SET attempt = $1, status = $2, next_attempt_at = $3, delivered_at = $4 WHERE id = $5",
		)
		.bind(attempt)
		.bind(status)
		.bind(next_attempt_at)
		.bind(delivered_at)
		.bind(id)
		.execute(&self.pool)
		.await?;
		Ok(())
	}
}

fn webhook_from_row(row: sqlx::any::AnyRow) -> WebhookRow {
	WebhookRow {
		id: row.get("id"),
		url: row.get("url"),
		event_kinds: row.get("event_kinds"),
		created_at: row.get("created_at"),
	}
}
