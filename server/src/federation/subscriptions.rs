use sqlx::Row;

use crate::db::MetadataStore;

pub struct DefinitionSubscriptionRow {
	pub home_url: String,
	pub id: String,
	pub kind: String,
	pub updated_at: i64,
}

impl MetadataStore {
	pub async fn upsert_definition_subscription(
		&self,
		home_url: &str,
		id: &str,
		kind: &str,
		updated_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO definition_subscriptions (home_url, id, kind, updated_at) VALUES ($1, $2, $3, $4)
			 ON CONFLICT(home_url, id) DO UPDATE SET kind = $3, updated_at = $4",
		)
		.bind(home_url)
		.bind(id)
		.bind(kind)
		.bind(updated_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn definition_subscriptions(&self) -> Result<Vec<DefinitionSubscriptionRow>, sqlx::Error> {
		let rows = sqlx::query("SELECT home_url, id, kind, updated_at FROM definition_subscriptions ORDER BY home_url, id")
			.fetch_all(&self.pool)
			.await?;
		Ok(rows
			.into_iter()
			.map(|row| DefinitionSubscriptionRow {
				home_url: row.get("home_url"),
				id: row.get("id"),
				kind: row.get("kind"),
				updated_at: row.get("updated_at"),
			})
			.collect())
	}
}

pub struct SubscriptionRow {
	pub home_url: String,
	pub project_id: String,
	pub cursor_seq: i64,
	pub remote_head_seq: i64,
	pub reset_count: i64,
	pub status: String,
	pub updated_at: i64,
}

fn subscription_from_row(row: sqlx::any::AnyRow) -> SubscriptionRow {
	SubscriptionRow {
		home_url: row.get("home_url"),
		project_id: row.get("project_id"),
		cursor_seq: row.get("cursor_seq"),
		remote_head_seq: row.get("remote_head_seq"),
		reset_count: row.get("reset_count"),
		status: row.get("status"),
		updated_at: row.get("updated_at"),
	}
}

impl MetadataStore {
	pub async fn upsert_subscription(
		&self,
		home_url: &str,
		project_id: &str,
		status: &str,
		updated_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO subscriptions (home_url, project_id, cursor_seq, status, updated_at) VALUES ($1, $2, 0, $3, $4)
			 ON CONFLICT(home_url, project_id) DO UPDATE SET status = $3, updated_at = $4",
		)
		.bind(home_url)
		.bind(project_id)
		.bind(status)
		.bind(updated_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn subscription(&self, home_url: &str, project_id: &str) -> Result<Option<SubscriptionRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT home_url, project_id, cursor_seq, remote_head_seq, reset_count, status, updated_at FROM subscriptions WHERE home_url = $1 AND project_id = $2",
		)
		.bind(home_url)
		.bind(project_id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(subscription_from_row))
	}

	pub async fn reset_subscription(
		&self,
		home_url: &str,
		project_id: &str,
		cursor_seq: i64,
		updated_at: i64,
	) -> Result<bool, sqlx::Error> {
		let result = sqlx::query(
			"UPDATE subscriptions SET cursor_seq = $1, remote_head_seq = $1, reset_count = reset_count + 1, updated_at = $2
			 WHERE home_url = $3 AND project_id = $4",
		)
		.bind(cursor_seq)
		.bind(updated_at)
		.bind(home_url)
		.bind(project_id)
		.execute(&self.pool)
		.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn set_subscription_cursor(
		&self,
		home_url: &str,
		project_id: &str,
		cursor_seq: i64,
		remote_head_seq: i64,
		status: &str,
		updated_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"UPDATE subscriptions SET cursor_seq = $1, remote_head_seq = $2, status = $3, updated_at = $4
			 WHERE home_url = $5 AND project_id = $6",
		)
		.bind(cursor_seq)
		.bind(remote_head_seq)
		.bind(status)
		.bind(updated_at)
		.bind(home_url)
		.bind(project_id)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn remove_subscription(&self, home_url: &str, project_id: &str) -> Result<bool, sqlx::Error> {
		let result = sqlx::query("DELETE FROM subscriptions WHERE home_url = $1 AND project_id = $2")
			.bind(home_url)
			.bind(project_id)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn subscriptions(&self) -> Result<Vec<SubscriptionRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT home_url, project_id, cursor_seq, remote_head_seq, reset_count, status, updated_at FROM subscriptions ORDER BY home_url, project_id",
		)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(subscription_from_row).collect())
	}
}

#[cfg(test)]
mod subscription_tests {
	use crate::db::MetadataStore;

	#[tokio::test]
	async fn reports_the_furthest_unapplied_entry() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		store
			.upsert_subscription("https://home", "p", "active", 1)
			.await
			.expect("subscribe");
		store
			.set_subscription_cursor("https://home", "p", 3, 5, "active", 2)
			.await
			.expect("cursor");

		let snapshot = store.metrics_snapshot().await.expect("snapshot");

		assert_eq!(snapshot.subscription_lag, 2);
	}
}
