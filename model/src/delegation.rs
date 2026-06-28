use moraine_codec::Value;
use moraine_crypto::KeyId;

use crate::canonical::{
	Canonical, Fields, expect_bytes, expect_i64, expect_text, expect_text_array, expect_u32, expect_u64, map_of,
};
use crate::error::{ModelError, RejectReason};
use crate::genesis::RootKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegationPurpose {
	Key,
	OwnershipTransfer,
}

impl DelegationPurpose {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Key => "key",
			Self::OwnershipTransfer => "ownership-transfer",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"key" => Self::Key,
			"ownership-transfer" => Self::OwnershipTransfer,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerRef {
	pub kind: String,
	pub id: String,
}

impl OwnerRef {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.kind != "user" && self.kind != "org" {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "owner kind"));
		}
		if self.id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "owner id"));
		}
		Ok(())
	}
}

impl Canonical for OwnerRef {
	fn to_value(&self) -> Value {
		map_of(
			"OwnerRef",
			[("kind", Value::text(self.kind.clone())), ("id", Value::text(self.id.clone()))],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("OwnerRef", value)?.reject_unknown(&["kind", "id"])?;
		let owner = Self {
			kind: expect_text(fields.required("kind")?, "kind")?,
			id: expect_text(fields.required("id")?, "id")?,
		};
		owner.validate()?;
		Ok(owner)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delegation {
	pub protocol: u32,
	pub purpose: DelegationPurpose,
	pub project_id: String,
	pub delegate_key: Option<RootKey>,
	pub allowed_kinds: Option<Vec<String>>,
	pub channels: Option<Vec<String>>,
	pub max_version_scope: Option<String>,
	pub valid_from_seq: Option<u64>,
	pub expires_at: Option<i64>,
	pub from_owner: Option<OwnerRef>,
	pub to_owner: Option<OwnerRef>,
	pub issued_at: i64,
	pub previous_delegation_digest: Option<Vec<u8>>,
}

impl Delegation {
	pub fn delegate_key_id(&self) -> Option<KeyId> {
		self.delegate_key.as_ref().map(|key| key.key_id)
	}

	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		match self.purpose {
			DelegationPurpose::Key => {
				if self.delegate_key.is_none() {
					return Err(ModelError::field(RejectReason::MissingField, "delegate_key"));
				}
				match &self.allowed_kinds {
					Some(kinds) if kinds.is_empty() => {
						return Err(ModelError::field(RejectReason::InvalidFieldValue, "allowed_kinds"));
					}
					None => return Err(ModelError::field(RejectReason::MissingField, "allowed_kinds")),
					_ => {}
				}
			}
			DelegationPurpose::OwnershipTransfer => {
				let from = self
					.from_owner
					.as_ref()
					.ok_or_else(|| ModelError::field(RejectReason::MissingField, "from_owner"))?;
				let to = self
					.to_owner
					.as_ref()
					.ok_or_else(|| ModelError::field(RejectReason::MissingField, "to_owner"))?;
				from.validate()?;
				to.validate()?;
			}
		}
		Ok(())
	}
}

impl Canonical for Delegation {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("purpose", Value::text(self.purpose.as_str())),
			("project_id", Value::text(self.project_id.clone())),
		];
		if let Some(delegate_key) = &self.delegate_key {
			pairs.push(("delegate_key", delegate_key.to_value()));
		}
		if let Some(kinds) = &self.allowed_kinds {
			pairs.push((
				"allowed_kinds",
				Value::array(kinds.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			));
		}
		if let Some(channels) = &self.channels {
			pairs.push((
				"channels",
				Value::array(channels.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			));
		}
		if let Some(scope) = &self.max_version_scope {
			pairs.push(("max_version_scope", Value::text(scope.clone())));
		}
		if let Some(sequence) = self.valid_from_seq {
			pairs.push(("valid_from_seq", Value::int(sequence as i64)));
		}
		if let Some(expires_at) = self.expires_at {
			pairs.push(("expires_at", Value::int(expires_at)));
		}
		if let Some(from_owner) = &self.from_owner {
			pairs.push(("from_owner", from_owner.to_value()));
		}
		if let Some(to_owner) = &self.to_owner {
			pairs.push(("to_owner", to_owner.to_value()));
		}
		pairs.push(("issued_at", Value::int(self.issued_at)));
		if let Some(previous) = &self.previous_delegation_digest {
			pairs.push(("previous_delegation_digest", Value::bytes(previous.clone())));
		}
		map_of("Delegation", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Delegation", value)?.reject_unknown(&[
			"protocol",
			"purpose",
			"project_id",
			"delegate_key",
			"allowed_kinds",
			"channels",
			"max_version_scope",
			"valid_from_seq",
			"expires_at",
			"from_owner",
			"to_owner",
			"issued_at",
			"previous_delegation_digest",
		])?;
		let purpose_text = expect_text(fields.required("purpose")?, "purpose")?;
		let purpose = DelegationPurpose::parse(&purpose_text)
			.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "purpose"))?;
		let delegation = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			purpose,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			delegate_key: fields
				.optional("delegate_key")
				.cloned()
				.map(RootKey::from_value)
				.transpose()?,
			allowed_kinds: fields
				.optional("allowed_kinds")
				.map(|v| expect_text_array(v, "allowed_kinds"))
				.transpose()?,
			channels: fields
				.optional("channels")
				.map(|v| expect_text_array(v, "channels"))
				.transpose()?,
			max_version_scope: fields
				.optional("max_version_scope")
				.map(|v| expect_text(v, "max_version_scope"))
				.transpose()?,
			valid_from_seq: fields
				.optional("valid_from_seq")
				.map(|v| expect_u64(v, "valid_from_seq"))
				.transpose()?,
			expires_at: fields
				.optional("expires_at")
				.map(|v| expect_i64(v, "expires_at"))
				.transpose()?,
			from_owner: fields.optional("from_owner").cloned().map(OwnerRef::from_value).transpose()?,
			to_owner: fields.optional("to_owner").cloned().map(OwnerRef::from_value).transpose()?,
			issued_at: expect_i64(fields.required("issued_at")?, "issued_at")?,
			previous_delegation_digest: fields
				.optional("previous_delegation_digest")
				.map(|v| expect_bytes(v, "previous_delegation_digest"))
				.transpose()?,
		};
		delegation.validate()?;
		Ok(delegation)
	}
}
