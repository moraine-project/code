use sha2::{Digest, Sha256};

use super::now;
use super::store::NotificationRow;
use crate::routes::AppState;

pub(crate) async fn notify_followers(
	state: &AppState,
	project_id: &str,
	event_kind: &str,
	object_digest: &[u8],
	feed_seq: i64,
) -> Result<(), sqlx::Error> {
	let followers = state.metadata.followers(project_id).await?;
	for user_id in followers {
		let notification = NotificationRow {
			id: notification_id(&user_id, project_id, feed_seq),
			project_id: project_id.to_string(),
			event_kind: event_kind.to_string(),
			object_digest: Some(object_digest.to_vec()),
			feed_seq: Some(feed_seq),
			created_at: now(),
			read_at: None,
		};
		state.metadata.insert_notification(&notification, &user_id).await?;
	}
	Ok(())
}

fn notification_id(user_id: &str, project_id: &str, feed_seq: i64) -> String {
	let mut value = Vec::new();
	value.extend_from_slice(project_id.as_bytes());
	value.push(0);
	value.extend_from_slice(&feed_seq.to_be_bytes());
	value.push(0);
	value.extend_from_slice(user_id.as_bytes());
	hex::encode(Sha256::digest(value))
}
