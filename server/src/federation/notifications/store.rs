use sqlx::Row;

use crate::db::MetadataStore;

#[derive(Debug, Clone)]
pub struct NotificationRow {
	pub id: String,
	pub project_id: String,
	pub event_kind: String,
	pub object_digest: Option<Vec<u8>>,
	pub feed_seq: Option<i64>,
	pub created_at: i64,
	pub read_at: Option<i64>,
}

impl MetadataStore {
	pub async fn follow(&self, user_id: &str, project_id: &str, created_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT INTO follows (user_id, project_id, created_at) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
			.bind(user_id)
			.bind(project_id)
			.bind(created_at)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn unfollow(&self, user_id: &str, project_id: &str) -> Result<bool, sqlx::Error> {
		let result = sqlx::query("DELETE FROM follows WHERE user_id = $1 AND project_id = $2")
			.bind(user_id)
			.bind(project_id)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn follows(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
		let rows = sqlx::query("SELECT project_id FROM follows WHERE user_id = $1 ORDER BY created_at")
			.bind(user_id)
			.fetch_all(&self.pool)
			.await?;
		Ok(rows.into_iter().map(|row| row.get("project_id")).collect())
	}

	pub async fn followers(&self, project_id: &str) -> Result<Vec<String>, sqlx::Error> {
		let rows = sqlx::query("SELECT user_id FROM follows WHERE project_id = $1")
			.bind(project_id)
			.fetch_all(&self.pool)
			.await?;
		Ok(rows.into_iter().map(|row| row.get("user_id")).collect())
	}

	pub async fn insert_notification(&self, notification: &NotificationRow, user_id: &str) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO notifications (id, user_id, project_id, event_kind, object_digest, feed_seq, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT DO NOTHING",
		)
		.bind(&notification.id)
		.bind(user_id)
		.bind(&notification.project_id)
		.bind(&notification.event_kind)
		.bind(&notification.object_digest)
		.bind(notification.feed_seq)
		.bind(notification.created_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn notifications(
		&self,
		user_id: &str,
		unread_only: bool,
		limit: i64,
	) -> Result<Vec<NotificationRow>, sqlx::Error> {
		let sql = if unread_only {
			"SELECT id, project_id, event_kind, object_digest, feed_seq, created_at, read_at FROM notifications WHERE user_id = $1 AND read_at IS NULL ORDER BY created_at DESC LIMIT $2"
		} else {
			"SELECT id, project_id, event_kind, object_digest, feed_seq, created_at, read_at FROM notifications WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2"
		};
		let rows = sqlx::query(sql).bind(user_id).bind(limit).fetch_all(&self.pool).await?;
		Ok(rows
			.into_iter()
			.map(|row| NotificationRow {
				id: row.get("id"),
				project_id: row.get("project_id"),
				event_kind: row.get("event_kind"),
				object_digest: row.get("object_digest"),
				feed_seq: row.get("feed_seq"),
				created_at: row.get("created_at"),
				read_at: row.get("read_at"),
			})
			.collect())
	}

	pub async fn mark_notification_read(&self, user_id: &str, id: &str, read_at: i64) -> Result<bool, sqlx::Error> {
		let result = sqlx::query("UPDATE notifications SET read_at = $1 WHERE id = $2 AND user_id = $3 AND read_at IS NULL")
			.bind(read_at)
			.bind(id)
			.bind(user_id)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn mark_all_notifications_read(&self, user_id: &str, read_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE notifications SET read_at = $1 WHERE user_id = $2 AND read_at IS NULL")
			.bind(read_at)
			.bind(user_id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn prune_notifications(&self, before: i64) -> Result<u64, sqlx::Error> {
		let result = sqlx::query("DELETE FROM notifications WHERE created_at < $1")
			.bind(before)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected())
	}
}
