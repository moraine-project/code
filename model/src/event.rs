use moraine_codec::Value;

use crate::canonical::{Canonical, Fields, expect_bytes, expect_i64, expect_text, expect_u32, expect_u64, map_of};
use crate::error::{ModelError, RejectReason};

pub const WEBHOOK_DOMAIN: &[u8] = b"GAMEDIST/v1/webhook\0";

pub const NOTIFICATION_RETENTION_DAYS: u64 = 90;
pub const WEBHOOK_RETENTION_DAYS: u64 = 30;
pub const COUNT_WINDOW_DAYS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
	ReleasePublished,
	ReleaseWithdrawn,
	ProfileUpdated,
	KeyChanged,
	Recovery,
	Migration,
	Advisory,
	ForkDetected,
	OwnershipTransferred,
}

impl EventKind {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::ReleasePublished => "release-published",
			Self::ReleaseWithdrawn => "release-withdrawn",
			Self::ProfileUpdated => "profile-updated",
			Self::KeyChanged => "key-changed",
			Self::Recovery => "recovery",
			Self::Migration => "migration",
			Self::Advisory => "advisory",
			Self::ForkDetected => "fork-detected",
			Self::OwnershipTransferred => "ownership-transferred",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"release-published" => Self::ReleasePublished,
			"release-withdrawn" => Self::ReleaseWithdrawn,
			"profile-updated" => Self::ProfileUpdated,
			"key-changed" => Self::KeyChanged,
			"recovery" => Self::Recovery,
			"migration" => Self::Migration,
			"advisory" => Self::Advisory,
			"fork-detected" => Self::ForkDetected,
			_ => return None,
		})
	}

	pub const fn is_feed_derived(self) -> bool {
		matches!(
			self,
			Self::ReleasePublished
				| Self::ReleaseWithdrawn
				| Self::ProfileUpdated
				| Self::KeyChanged
				| Self::Migration
				| Self::Recovery
				| Self::OwnershipTransferred
		)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
	pub protocol: u32,
	pub event_id: String,
	pub event_kind: EventKind,
	pub project_id: String,
	pub game_id: Option<String>,
	pub feed_seq: Option<u64>,
	pub object_digest: Option<Vec<u8>>,
	pub advisory_digest: Option<Vec<u8>>,
	pub issued_at: i64,
}

impl Event {
	pub fn signing_message(&self) -> Vec<u8> {
		let mut message = WEBHOOK_DOMAIN.to_vec();
		message.extend_from_slice(&self.to_canonical_bytes());
		message
	}

	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.event_id.is_empty() || self.project_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "event_id"));
		}
		if self.event_kind.is_feed_derived() && self.feed_seq.is_none() {
			return Err(ModelError::field(RejectReason::MissingField, "feed_seq"));
		}
		if self.event_kind == EventKind::Advisory && self.advisory_digest.is_none() {
			return Err(ModelError::field(RejectReason::MissingField, "advisory_digest"));
		}
		if let Some(digest) = &self.object_digest
			&& digest.len() != 32
		{
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "object_digest"));
		}
		if let Some(digest) = &self.advisory_digest
			&& digest.len() != 32
		{
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "advisory_digest"));
		}
		Ok(())
	}
}

impl Canonical for Event {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("event_id", Value::text(self.event_id.clone())),
			("event_kind", Value::text(self.event_kind.as_str())),
			("project_id", Value::text(self.project_id.clone())),
		];
		if let Some(game_id) = &self.game_id {
			pairs.push(("game_id", Value::text(game_id.clone())));
		}
		if let Some(feed_seq) = self.feed_seq {
			pairs.push(("feed_seq", Value::int(feed_seq as i64)));
		}
		if let Some(digest) = &self.object_digest {
			pairs.push(("object_digest", Value::bytes(digest.clone())));
		}
		if let Some(digest) = &self.advisory_digest {
			pairs.push(("advisory_digest", Value::bytes(digest.clone())));
		}
		pairs.push(("issued_at", Value::int(self.issued_at)));
		map_of("Event", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Event", value)?.reject_unknown(&[
			"protocol",
			"event_id",
			"event_kind",
			"project_id",
			"game_id",
			"feed_seq",
			"object_digest",
			"advisory_digest",
			"issued_at",
		])?;
		let event_kind_text = expect_text(fields.required("event_kind")?, "event_kind")?;
		let event = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			event_id: expect_text(fields.required("event_id")?, "event_id")?,
			event_kind: EventKind::parse(&event_kind_text)
				.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "event_kind"))?,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			game_id: fields
				.optional("game_id")
				.map(|value| expect_text(value, "game_id"))
				.transpose()?,
			feed_seq: fields.optional("feed_seq").map(|v| expect_u64(v, "feed_seq")).transpose()?,
			object_digest: fields
				.optional("object_digest")
				.map(|v| expect_bytes(v, "object_digest"))
				.transpose()?,
			advisory_digest: fields
				.optional("advisory_digest")
				.map(|v| expect_bytes(v, "advisory_digest"))
				.transpose()?,
			issued_at: expect_i64(fields.required("issued_at")?, "issued_at")?,
		};
		event.validate()?;
		Ok(event)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn feed_derived_events_require_a_sequence() {
		let event = Event {
			protocol: 1,
			event_id: "e1".to_string(),
			event_kind: EventKind::ReleasePublished,
			project_id: "p".to_string(),
			game_id: Some("g".to_string()),
			feed_seq: None,
			object_digest: None,
			advisory_digest: None,
			issued_at: 1,
		};
		assert_eq!(event.validate().unwrap_err().reason, RejectReason::MissingField);
	}
}
