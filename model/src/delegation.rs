use moraine_codec::Value;
use moraine_crypto::KeyId;

use crate::canonical::{
	Canonical, Fields, canonical_i64, expect_array, expect_bytes, expect_canonical_u64, expect_i64, expect_text,
	expect_text_array, expect_u32, expect_u64, map_of,
};
use crate::error::{ModelError, RejectReason};
use crate::genesis::RootKey;
use crate::signed::ALG_ED25519;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegationPurpose {
	Key,
	OwnershipTransfer,
	Migration,
	Recovery,
}

impl DelegationPurpose {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Key => "key",
			Self::OwnershipTransfer => "ownership-transfer",
			Self::Migration => "migration",
			Self::Recovery => "recovery",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"key" => Self::Key,
			"ownership-transfer" => Self::OwnershipTransfer,
			"migration" => Self::Migration,
			"recovery" => Self::Recovery,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerRef {
	pub kind: String,
	pub id: String,
	pub key_id: KeyId,
}

impl OwnerRef {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.kind != "user" && self.kind != "org" {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "owner kind"));
		}
		if self.id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "owner id"));
		}
		if self.key_id.algorithm() != ALG_ED25519 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "owner key id"));
		}
		Ok(())
	}
}

impl Canonical for OwnerRef {
	fn to_value(&self) -> Value {
		map_of(
			"OwnerRef",
			[
				("kind", Value::text(self.kind.clone())),
				("id", Value::text(self.id.clone())),
				("key_id", Value::bytes(self.key_id.digest())),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("OwnerRef", value)?.reject_unknown(&["kind", "id", "key_id"])?;
		let owner = Self {
			kind: expect_text(fields.required("kind")?, "kind")?,
			id: expect_text(fields.required("id")?, "id")?,
			key_id: expect_key_id(fields.required("key_id")?, "key_id")?,
		};
		owner.validate()?;
		Ok(owner)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseWindow {
	pub from_seq: u64,
	pub to_seq: u64,
}

impl ReleaseWindow {
	pub fn validate(&self) -> Result<(), ModelError> {
		expect_canonical_u64(self.from_seq, "from_seq")?;
		expect_canonical_u64(self.to_seq, "to_seq")?;
		if self.from_seq > self.to_seq {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "from_seq"));
		}
		Ok(())
	}
}

impl Canonical for ReleaseWindow {
	fn to_value(&self) -> Value {
		map_of(
			"ReleaseWindow",
			[
				("from_seq", Value::int(canonical_i64(self.from_seq, "from_seq"))),
				("to_seq", Value::int(canonical_i64(self.to_seq, "to_seq"))),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("ReleaseWindow", value)?.reject_unknown(&["from_seq", "to_seq"])?;
		let window = Self {
			from_seq: expect_u64(fields.required("from_seq")?, "from_seq")?,
			to_seq: expect_u64(fields.required("to_seq")?, "to_seq")?,
		};
		window.validate()?;
		Ok(window)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyDelegation {
	pub protocol: u32,
	pub project_id: String,
	pub delegate_key: RootKey,
	pub allowed_kinds: Vec<String>,
	pub channels: Option<Vec<String>>,
	pub max_version_scope: Option<String>,
	pub valid_from_seq: Option<u64>,
	pub expires_at: Option<i64>,
	pub issued_at: i64,
	pub previous_delegation_digest: Option<Vec<u8>>,
}

impl KeyDelegation {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.project_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "project_id"));
		}
		if self.allowed_kinds.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "allowed_kinds"));
		}
		if let Some(sequence) = self.valid_from_seq {
			expect_canonical_u64(sequence, "valid_from_seq")?;
		}
		Ok(())
	}
}

impl Canonical for KeyDelegation {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("purpose", Value::text(DelegationPurpose::Key.as_str())),
			("project_id", Value::text(self.project_id.clone())),
			("delegate_key", self.delegate_key.to_value()),
			(
				"allowed_kinds",
				Value::array(self.allowed_kinds.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			),
		];
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
			pairs.push(("valid_from_seq", Value::int(canonical_i64(sequence, "valid_from_seq"))));
		}
		if let Some(expires_at) = self.expires_at {
			pairs.push(("expires_at", Value::int(expires_at)));
		}
		pairs.push(("issued_at", Value::int(self.issued_at)));
		if let Some(previous) = &self.previous_delegation_digest {
			pairs.push(("previous_delegation_digest", Value::bytes(previous.clone())));
		}
		map_of("KeyDelegation", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("KeyDelegation", value)?.reject_unknown(&[
			"protocol",
			"purpose",
			"project_id",
			"delegate_key",
			"allowed_kinds",
			"channels",
			"max_version_scope",
			"valid_from_seq",
			"expires_at",
			"issued_at",
			"previous_delegation_digest",
		])?;
		expect_purpose(&fields, DelegationPurpose::Key)?;
		let delegate_key = RootKey::from_value(fields.required("delegate_key")?.clone())?;
		let derived = moraine_crypto::key_id(ALG_ED25519, &delegate_key.public_key)
			.map_err(|_| ModelError::new(RejectReason::InvalidFieldValue, "invalid delegate public key"))?;
		if derived != delegate_key.key_id {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"delegate key_id does not match its public key",
			));
		}
		let delegation = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			delegate_key,
			allowed_kinds: expect_text_array(fields.required("allowed_kinds")?, "allowed_kinds")?,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnershipTransfer {
	pub protocol: u32,
	pub project_id: String,
	pub from_owner: OwnerRef,
	pub to_owner: OwnerRef,
	pub issued_at: i64,
	pub previous_delegation_digest: Option<Vec<u8>>,
}

impl OwnershipTransfer {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.project_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "project_id"));
		}
		self.from_owner.validate()?;
		self.to_owner.validate()?;
		if self.from_owner == self.to_owner {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "to_owner"));
		}
		if self.from_owner.key_id == self.to_owner.key_id {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "to_owner"));
		}
		Ok(())
	}
}

impl Canonical for OwnershipTransfer {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("purpose", Value::text(DelegationPurpose::OwnershipTransfer.as_str())),
			("project_id", Value::text(self.project_id.clone())),
			("from_owner", self.from_owner.to_value()),
			("to_owner", self.to_owner.to_value()),
			("issued_at", Value::int(self.issued_at)),
		];
		if let Some(previous) = &self.previous_delegation_digest {
			pairs.push(("previous_delegation_digest", Value::bytes(previous.clone())));
		}
		map_of("OwnershipTransfer", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("OwnershipTransfer", value)?.reject_unknown(&[
			"protocol",
			"purpose",
			"project_id",
			"from_owner",
			"to_owner",
			"issued_at",
			"previous_delegation_digest",
		])?;
		expect_purpose(&fields, DelegationPurpose::OwnershipTransfer)?;
		let transfer = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			from_owner: OwnerRef::from_value(fields.required("from_owner")?.clone())?,
			to_owner: OwnerRef::from_value(fields.required("to_owner")?.clone())?,
			issued_at: expect_i64(fields.required("issued_at")?, "issued_at")?,
			previous_delegation_digest: fields
				.optional("previous_delegation_digest")
				.map(|v| expect_bytes(v, "previous_delegation_digest"))
				.transpose()?,
		};
		transfer.validate()?;
		Ok(transfer)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
	pub protocol: u32,
	pub project_id: String,
	pub old_home: String,
	pub new_home: String,
	pub cutover_seq: u64,
	pub reason: Option<String>,
	pub declared_time: i64,
}

impl Migration {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.project_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "project_id"));
		}
		if self.old_home.is_empty() || self.new_home.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "new_home"));
		}
		if self.old_home == self.new_home {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "new_home"));
		}
		expect_canonical_u64(self.cutover_seq, "cutover_seq")?;
		Ok(())
	}
}

impl Canonical for Migration {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("purpose", Value::text(DelegationPurpose::Migration.as_str())),
			("project_id", Value::text(self.project_id.clone())),
			("old_home", Value::text(self.old_home.clone())),
			("new_home", Value::text(self.new_home.clone())),
			("cutover_seq", Value::int(canonical_i64(self.cutover_seq, "cutover_seq"))),
		];
		if let Some(reason) = &self.reason {
			pairs.push(("reason", Value::text(reason.clone())));
		}
		pairs.push(("declared_time", Value::int(self.declared_time)));
		map_of("Migration", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Migration", value)?.reject_unknown(&[
			"protocol",
			"purpose",
			"project_id",
			"old_home",
			"new_home",
			"cutover_seq",
			"reason",
			"declared_time",
		])?;
		expect_purpose(&fields, DelegationPurpose::Migration)?;
		let migration = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			old_home: expect_text(fields.required("old_home")?, "old_home")?,
			new_home: expect_text(fields.required("new_home")?, "new_home")?,
			cutover_seq: expect_u64(fields.required("cutover_seq")?, "cutover_seq")?,
			reason: fields.optional("reason").map(|v| expect_text(v, "reason")).transpose()?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		migration.validate()?;
		Ok(migration)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryEvent {
	pub protocol: u32,
	pub project_id: String,
	pub compromised_key_ids: Vec<KeyId>,
	pub valid_from_seq: u64,
	pub replacement_roots: Vec<RootKey>,
	pub affected_release_window: ReleaseWindow,
	pub reason: String,
	pub declared_time: i64,
}

impl RecoveryEvent {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.project_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "project_id"));
		}
		if self.compromised_key_ids.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "compromised_key_ids"));
		}
		if self.replacement_roots.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "replacement_roots"));
		}
		expect_canonical_u64(self.valid_from_seq, "valid_from_seq")?;
		self.affected_release_window.validate()?;
		for root in &self.replacement_roots {
			let derived = moraine_crypto::key_id(ALG_ED25519, &root.public_key)
				.map_err(|_| ModelError::new(RejectReason::InvalidFieldValue, "invalid replacement key"))?;
			if derived != root.key_id {
				return Err(ModelError::new(
					RejectReason::InvalidFieldValue,
					"replacement root key_id does not match its public key",
				));
			}
		}
		Ok(())
	}
}

impl Canonical for RecoveryEvent {
	fn to_value(&self) -> Value {
		map_of(
			"RecoveryEvent",
			[
				("protocol", Value::int(i64::from(self.protocol))),
				("purpose", Value::text(DelegationPurpose::Recovery.as_str())),
				("project_id", Value::text(self.project_id.clone())),
				(
					"compromised_key_ids",
					Value::array(
						self.compromised_key_ids
							.iter()
							.map(|id| Value::bytes(id.digest()))
							.collect::<Vec<_>>(),
					),
				),
				(
					"valid_from_seq",
					Value::int(canonical_i64(self.valid_from_seq, "valid_from_seq")),
				),
				(
					"replacement_roots",
					Value::array(self.replacement_roots.iter().map(Canonical::to_value).collect::<Vec<_>>()),
				),
				("affected_release_window", self.affected_release_window.to_value()),
				("reason", Value::text(self.reason.clone())),
				("declared_time", Value::int(self.declared_time)),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("RecoveryEvent", value)?.reject_unknown(&[
			"protocol",
			"purpose",
			"project_id",
			"compromised_key_ids",
			"valid_from_seq",
			"replacement_roots",
			"affected_release_window",
			"reason",
			"declared_time",
		])?;
		expect_purpose(&fields, DelegationPurpose::Recovery)?;
		let compromised_key_ids = expect_array(fields.required("compromised_key_ids")?, "compromised_key_ids")?
			.iter()
			.map(|value| {
				let digest: [u8; 32] = expect_bytes(value, "compromised_key_ids")?
					.try_into()
					.map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, "compromised_key_ids"))?;
				Ok(KeyId::from_digest(ALG_ED25519, digest))
			})
			.collect::<Result<Vec<_>, ModelError>>()?;
		let recovery = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			compromised_key_ids,
			valid_from_seq: expect_u64(fields.required("valid_from_seq")?, "valid_from_seq")?,
			replacement_roots: expect_array(fields.required("replacement_roots")?, "replacement_roots")?
				.iter()
				.cloned()
				.map(RootKey::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			affected_release_window: ReleaseWindow::from_value(fields.required("affected_release_window")?.clone())?,
			reason: expect_text(fields.required("reason")?, "reason")?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		recovery.validate()?;
		Ok(recovery)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delegation {
	Key(KeyDelegation),
	OwnershipTransfer(OwnershipTransfer),
	Migration(Migration),
	Recovery(RecoveryEvent),
}

impl Delegation {
	pub fn validate(&self) -> Result<(), ModelError> {
		match self {
			Self::Key(delegation) => delegation.validate(),
			Self::OwnershipTransfer(transfer) => transfer.validate(),
			Self::Migration(migration) => migration.validate(),
			Self::Recovery(recovery) => recovery.validate(),
		}
	}
}

impl Delegation {
	pub const fn purpose(&self) -> DelegationPurpose {
		match self {
			Self::Key(_) => DelegationPurpose::Key,
			Self::OwnershipTransfer(_) => DelegationPurpose::OwnershipTransfer,
			Self::Migration(_) => DelegationPurpose::Migration,
			Self::Recovery(_) => DelegationPurpose::Recovery,
		}
	}
}

impl Canonical for Delegation {
	fn to_value(&self) -> Value {
		match self {
			Self::Key(delegation) => delegation.to_value(),
			Self::OwnershipTransfer(transfer) => transfer.to_value(),
			Self::Migration(migration) => migration.to_value(),
			Self::Recovery(recovery) => recovery.to_value(),
		}
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let purpose = value
			.get("purpose")
			.and_then(Value::as_text)
			.ok_or_else(|| ModelError::field(RejectReason::MissingField, "purpose"))?;
		match DelegationPurpose::parse(purpose) {
			Some(DelegationPurpose::Key) => Ok(Self::Key(KeyDelegation::from_value(value)?)),
			Some(DelegationPurpose::OwnershipTransfer) => Ok(Self::OwnershipTransfer(OwnershipTransfer::from_value(value)?)),
			Some(DelegationPurpose::Migration) => Ok(Self::Migration(Migration::from_value(value)?)),
			Some(DelegationPurpose::Recovery) => Ok(Self::Recovery(RecoveryEvent::from_value(value)?)),
			None => Err(ModelError::field(RejectReason::InvalidFieldValue, "purpose")),
		}
	}
}

fn expect_key_id(value: &Value, key: &str) -> Result<KeyId, ModelError> {
	let digest: [u8; 32] = expect_bytes(value, key)?
		.try_into()
		.map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, key))?;
	Ok(KeyId::from_digest(ALG_ED25519, digest))
}

fn expect_purpose(fields: &Fields, expected: DelegationPurpose) -> Result<(), ModelError> {
	let found = expect_text(fields.required("purpose")?, "purpose")?;
	if found != expected.as_str() {
		return Err(ModelError::field(RejectReason::InvalidFieldValue, "purpose"));
	}
	Ok(())
}
