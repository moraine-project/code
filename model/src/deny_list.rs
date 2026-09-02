use moraine_codec::Value;

use crate::canonical::{Canonical, Fields, expect_array, expect_i64, expect_text, expect_u32, map_of};
use crate::error::{ModelError, RejectReason};
use crate::moderation::ScopeKind;

pub const MAX_DENY_LIST_ENTRIES: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyTarget {
	Project,
	ArtifactDigest,
}

impl DenyTarget {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Project => "project",
			Self::ArtifactDigest => "artifact-digest",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"project" => Self::Project,
			"artifact-digest" => Self::ArtifactDigest,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenyListEntry {
	pub target_kind: DenyTarget,
	pub target_id: String,
	pub reason_code: String,
	pub reason_taxonomy_version: u32,
	pub scope_kind: ScopeKind,
	pub scope_id: String,
	pub valid_from: Option<i64>,
	pub valid_until: Option<i64>,
}

impl DenyListEntry {
	pub fn is_active_at(&self, now: i64) -> bool {
		self.valid_from.is_none_or(|from| now >= from) && self.valid_until.is_none_or(|until| now < until)
	}

	fn validate(&self, index: usize) -> Result<(), ModelError> {
		if self.target_id.trim().is_empty() {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				format!("entries[{index}].target_id"),
			));
		}
		match self.target_kind {
			DenyTarget::Project if !self.target_id.starts_with("gd:sha256:") => {
				return Err(ModelError::new(
					RejectReason::InvalidFieldValue,
					format!("entries[{index}].target_id is not a project id"),
				));
			}
			DenyTarget::ArtifactDigest => {
				let hex = self.target_id.strip_prefix("sha256:").unwrap_or(&self.target_id);
				if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
					return Err(ModelError::new(
						RejectReason::InvalidFieldValue,
						format!("entries[{index}].target_id is not an artifact digest"),
					));
				}
			}
			DenyTarget::Project => {}
		}
		if self.reason_code.trim().is_empty() {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				format!("entries[{index}].reason_code"),
			));
		}
		if self.scope_id.trim().is_empty() {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				format!("entries[{index}].scope_id"),
			));
		}
		if let (Some(from), Some(until)) = (self.valid_from, self.valid_until)
			&& until <= from
		{
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				format!("entries[{index}].valid_until"),
			));
		}
		Ok(())
	}
}

impl Canonical for DenyListEntry {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("target_kind", Value::text(self.target_kind.as_str())),
			("target_id", Value::text(self.target_id.clone())),
			("reason_code", Value::text(self.reason_code.clone())),
			("reason_taxonomy_version", Value::int(i64::from(self.reason_taxonomy_version))),
			("scope_kind", Value::text(self.scope_kind.as_str())),
			("scope_id", Value::text(self.scope_id.clone())),
		];
		if let Some(from) = self.valid_from {
			pairs.push(("valid_from", Value::int(from)));
		}
		if let Some(until) = self.valid_until {
			pairs.push(("valid_until", Value::int(until)));
		}
		map_of("DenyListEntry", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("DenyListEntry", value)?.reject_unknown(&[
			"target_kind",
			"target_id",
			"reason_code",
			"reason_taxonomy_version",
			"scope_kind",
			"scope_id",
			"valid_from",
			"valid_until",
		])?;
		let target_text = expect_text(fields.required("target_kind")?, "target_kind")?;
		let scope_text = expect_text(fields.required("scope_kind")?, "scope_kind")?;
		Ok(Self {
			target_kind: DenyTarget::parse(&target_text)
				.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "target_kind"))?,
			target_id: expect_text(fields.required("target_id")?, "target_id")?,
			reason_code: expect_text(fields.required("reason_code")?, "reason_code")?,
			reason_taxonomy_version: expect_u32(fields.required("reason_taxonomy_version")?, "reason_taxonomy_version")?,
			scope_kind: ScopeKind::parse(&scope_text)
				.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "scope_kind"))?,
			scope_id: expect_text(fields.required("scope_id")?, "scope_id")?,
			valid_from: fields
				.optional("valid_from")
				.map(|v| expect_i64(v, "valid_from"))
				.transpose()?,
			valid_until: fields
				.optional("valid_until")
				.map(|v| expect_i64(v, "valid_until"))
				.transpose()?,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenyList {
	pub protocol: u32,
	pub issuer_id: String,
	pub entries: Vec<DenyListEntry>,
	pub issued_at: i64,
}

impl DenyList {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.issuer_id.trim().is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "issuer_id"));
		}
		if self.entries.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "entries"));
		}
		if self.entries.len() > MAX_DENY_LIST_ENTRIES {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"a deny list is bounded and holds at most 1000 entries",
			));
		}
		for (index, entry) in self.entries.iter().enumerate() {
			entry.validate(index)?;
		}
		Ok(())
	}

	pub fn active_entries(&self, now: i64) -> impl Iterator<Item = &DenyListEntry> {
		self.entries.iter().filter(move |entry| entry.is_active_at(now))
	}
}

impl Canonical for DenyList {
	fn to_value(&self) -> Value {
		map_of(
			"DenyList",
			[
				("protocol", Value::int(i64::from(self.protocol))),
				("issuer_id", Value::text(self.issuer_id.clone())),
				(
					"entries",
					Value::array(self.entries.iter().map(Canonical::to_value).collect::<Vec<_>>()),
				),
				("issued_at", Value::int(self.issued_at)),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("DenyList", value)?.reject_unknown(&["protocol", "issuer_id", "entries", "issued_at"])?;
		let list = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			issuer_id: expect_text(fields.required("issuer_id")?, "issuer_id")?,
			entries: expect_array(fields.required("entries")?, "entries")?
				.iter()
				.cloned()
				.map(DenyListEntry::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			issued_at: expect_i64(fields.required("issued_at")?, "issued_at")?,
		};
		list.validate()?;
		Ok(list)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn entry(target_kind: DenyTarget, target_id: &str) -> DenyListEntry {
		DenyListEntry {
			target_kind,
			target_id: target_id.to_string(),
			reason_code: "malware-confirmed".to_string(),
			reason_taxonomy_version: 1,
			scope_kind: ScopeKind::Instance,
			scope_id: "dir.example".to_string(),
			valid_from: None,
			valid_until: None,
		}
	}

	fn sample_list(entries: Vec<DenyListEntry>) -> DenyList {
		DenyList {
			protocol: 1,
			issuer_id: "dir.example".to_string(),
			entries,
			issued_at: 1_760_000_000,
		}
	}

	#[test]
	fn round_trips_and_bounds_a_list() {
		let list = sample_list(vec![entry(DenyTarget::Project, "gd:sha256:aa")]);
		let bytes = list.to_canonical_bytes();
		assert_eq!(DenyList::from_canonical_bytes(&bytes).expect("decode"), list);

		assert!(sample_list(vec![]).validate().is_err());
	}

	#[test]
	fn checks_target_shapes() {
		assert!(sample_list(vec![entry(DenyTarget::Project, "not-an-id")]).validate().is_err());
		assert!(
			sample_list(vec![entry(DenyTarget::ArtifactDigest, "sha256:00")])
				.validate()
				.is_err()
		);
		assert!(
			sample_list(vec![entry(DenyTarget::ArtifactDigest, &"ab".repeat(32))])
				.validate()
				.is_ok()
		);
	}

	#[test]
	fn an_entry_applies_only_inside_its_window() {
		let mut entry = entry(DenyTarget::Project, "gd:sha256:aa");
		entry.valid_from = Some(100);
		entry.valid_until = Some(200);
		assert!(!entry.is_active_at(99));
		assert!(entry.is_active_at(150));
		assert!(!entry.is_active_at(200));
	}
}
