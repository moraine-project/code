use moraine_codec::Value;
use moraine_crypto::{ObjectKind, object_id};

use crate::canonical::{Canonical, Fields, expect_bytes, expect_i64, expect_text, expect_u64, map_of};
use crate::error::{ModelError, RejectReason};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedEntry {
	pub protocol: u32,
	pub project_id: String,
	pub sequence: u64,
	pub previous: Option<Vec<u8>>,
	pub kind: String,
	pub object_digest: Vec<u8>,
	pub declared_at: i64,
}

impl FeedEntry {
	pub fn id_bytes(&self) -> [u8; 32] {
		object_id(ObjectKind::FeedEntry, &self.to_canonical_bytes())
	}

	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.sequence == 0 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "sequence"));
		}
		match (&self.previous, self.sequence) {
			(Some(previous), 1) if previous.len() == 32 => {
				return Err(ModelError::new(
					RejectReason::PreviousMismatch,
					"sequence 1 must not carry a previous digest",
				));
			}
			(None, 1) => {}
			(Some(previous), _) if previous.len() == 32 => {}
			(Some(_), _) => return Err(ModelError::field(RejectReason::InvalidFieldValue, "previous")),
			(None, _) => {
				return Err(ModelError::new(
					RejectReason::PreviousMismatch,
					"entry after sequence 1 must carry a previous digest",
				));
			}
		}
		if self.object_digest.len() != 32 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "object_digest"));
		}
		if self.kind.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "kind"));
		}
		Ok(())
	}

	pub fn verify_follows(&self, previous: &FeedEntry) -> Result<(), ModelError> {
		if self.sequence != previous.sequence + 1 {
			return Err(ModelError::new(
				RejectReason::SequenceGap,
				format!("expected sequence {}, found {}", previous.sequence + 1, self.sequence),
			));
		}
		match &self.previous {
			Some(digest) if digest.as_slice() == previous.id_bytes() => Ok(()),
			_ => Err(ModelError::new(
				RejectReason::PreviousMismatch,
				"previous digest does not match the prior entry",
			)),
		}
	}
}

impl Canonical for FeedEntry {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("project_id", Value::text(self.project_id.clone())),
			("sequence", Value::int(self.sequence as i64)),
		];
		if let Some(previous) = &self.previous {
			pairs.push(("previous", Value::bytes(previous.clone())));
		}
		pairs.push(("kind", Value::text(self.kind.clone())));
		pairs.push(("object_digest", Value::bytes(self.object_digest.clone())));
		pairs.push(("declared_at", Value::int(self.declared_at)));
		map_of("FeedEntry", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("FeedEntry", value)?.reject_unknown(&[
			"protocol",
			"project_id",
			"sequence",
			"previous",
			"kind",
			"object_digest",
			"declared_at",
		])?;
		let entry = Self {
			protocol: crate::canonical::expect_u32(fields.required("protocol")?, "protocol")?,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			sequence: expect_u64(fields.required("sequence")?, "sequence")?,
			previous: fields.optional("previous").map(|v| expect_bytes(v, "previous")).transpose()?,
			kind: expect_text(fields.required("kind")?, "kind")?,
			object_digest: expect_bytes(fields.required("object_digest")?, "object_digest")?,
			declared_at: expect_i64(fields.required("declared_at")?, "declared_at")?,
		};
		entry.validate()?;
		Ok(entry)
	}
}
