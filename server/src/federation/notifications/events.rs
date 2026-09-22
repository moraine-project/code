use super::store::NotificationRow;
use super::{new_id, now};
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
			id: new_id(),
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
