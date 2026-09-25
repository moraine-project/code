use moraine_model::Canonical;
use moraine_model::event::{Event, EventKind};
use sha2::{Digest, Sha256};

use super::{new_id, now};
use crate::routes::AppState;

pub(crate) async fn enqueue_event(
	state: &AppState,
	event_kind: &str,
	project_id: &str,
	object_digest: &[u8],
	feed_seq: i64,
) -> Result<(), sqlx::Error> {
	let Some(signer) = &state.capability.webhook_signer else {
		return Ok(());
	};
	let Some(kind) = EventKind::parse(event_kind) else {
		return Ok(());
	};
	let webhooks = state.metadata.active_webhooks().await?;
	if webhooks.is_empty() {
		return Ok(());
	}
	let event = Event {
		protocol: 1,
		event_id: feed_event_id(project_id, feed_seq, object_digest),
		event_kind: kind,
		project_id: project_id.to_string(),
		game_id: None,
		feed_seq: Some(feed_seq as u64),
		object_digest: Some(object_digest.to_vec()),
		advisory_digest: None,
		issued_at: now(),
	};
	let payload = event.to_canonical_bytes();
	let signature = signer.sign(&event.signing_message());
	let body = serde_json::json!({
		"protocol": 1,
		"event_id": event.event_id,
		"event_kind": event_kind,
		"project_id": project_id,
		"payload": hex::encode(&payload),
		"signature": hex::encode(signature),
	})
	.to_string();
	for webhook in webhooks {
		if !subscribes(&webhook.event_kinds, event_kind) {
			continue;
		}
		state
			.metadata
			.enqueue_delivery(&new_id(), &webhook.id, &event.event_id, &webhook.url, &body, now())
			.await?;
	}
	Ok(())
}

fn subscribes(event_kinds: &str, kind: &str) -> bool {
	event_kinds.is_empty() || event_kinds.split(',').any(|candidate| candidate == kind)
}

fn feed_event_id(project_id: &str, feed_seq: i64, object_digest: &[u8]) -> String {
	let mut value = Vec::new();
	value.extend_from_slice(project_id.as_bytes());
	value.push(0);
	value.extend_from_slice(&feed_seq.to_be_bytes());
	value.push(0);
	value.extend_from_slice(object_digest);
	hex::encode(Sha256::digest(value))
}
