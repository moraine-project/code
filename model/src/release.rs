use std::collections::BTreeMap;

use moraine_codec::Value;

use crate::artifact::Artifact;
use crate::canonical::{Canonical, Fields, expect_array, expect_bytes, expect_i64, expect_text, expect_u32, map_of};
use crate::compatibility::{Compatibility, Rights};
use crate::dependency::Dependency;
use crate::error::{ModelError, RejectReason};
use crate::location::LocationRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasePayload {
	pub protocol: u32,
	pub project_id: String,
	pub game_id: String,
	pub release_nonce: Vec<u8>,
	pub human_version: String,
	pub channel: String,
	pub kind: String,
	pub declared_time: i64,
	pub compatibility: Vec<Compatibility>,
	pub artifacts: Vec<Artifact>,
	pub dependencies: Vec<Dependency>,
	pub source_reference: Option<String>,
	pub changelog_digest: Option<Vec<u8>>,
	pub license_expression: Option<String>,
	pub rights: Option<Rights>,
	pub sbom_digest: Option<Vec<u8>>,
	pub minimum_verifier_version: u32,
	pub critical_extensions: Vec<String>,
}

impl ReleasePayload {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.release_nonce.len() < 16 {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"release_nonce must be at least 16 bytes",
			));
		}
		if self.project_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "project_id"));
		}
		if self.game_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "game_id"));
		}
		if self.artifacts.is_empty() {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"release must list at least one artifact",
			));
		}
		for artifact in &self.artifacts {
			artifact.validate()?;
		}
		if !self.critical_extensions.is_empty() {
			return Err(ModelError::new(
				RejectReason::UnknownCriticalExtension,
				format!("unrecognized critical extension `{}`", self.critical_extensions[0]),
			));
		}
		validate_primary_artifacts(&self.artifacts)?;
		Ok(())
	}
}

fn validate_primary_artifacts(artifacts: &[Artifact]) -> Result<(), ModelError> {
	let mut primaries: BTreeMap<(Vec<String>, Vec<String>), usize> = BTreeMap::new();
	for artifact in artifacts {
		let mut os = artifact.os_predicate.clone().unwrap_or_default();
		os.sort();
		let mut arch = artifact.arch_predicate.clone().unwrap_or_default();
		arch.sort();
		let slot = primaries.entry((os, arch)).or_default();
		if artifact.is_primary {
			*slot += 1;
		}
	}
	for count in primaries.values() {
		if *count != 1 {
			return Err(ModelError::new(
				RejectReason::PrimaryArtifactAmbiguous,
				"exactly one primary artifact is required per compatibility class",
			));
		}
	}
	Ok(())
}

impl Canonical for ReleasePayload {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("type", Value::text("release")),
			("project_id", Value::text(self.project_id.clone())),
			("game_id", Value::text(self.game_id.clone())),
			("release_nonce", Value::bytes(self.release_nonce.clone())),
			("human_version", Value::text(self.human_version.clone())),
			("channel", Value::text(self.channel.clone())),
			("kind", Value::text(self.kind.clone())),
			("declared_time", Value::int(self.declared_time)),
			(
				"compatibility",
				Value::array(self.compatibility.iter().map(Canonical::to_value).collect::<Vec<_>>()),
			),
			(
				"artifacts",
				Value::array(self.artifacts.iter().map(Canonical::to_value).collect::<Vec<_>>()),
			),
			(
				"dependencies",
				Value::array(self.dependencies.iter().map(Canonical::to_value).collect::<Vec<_>>()),
			),
		];
		if let Some(source_reference) = &self.source_reference {
			pairs.push(("source_reference", Value::text(source_reference.clone())));
		}
		if let Some(digest) = &self.changelog_digest {
			pairs.push(("changelog_digest", Value::bytes(digest.clone())));
		}
		if let Some(license) = &self.license_expression {
			pairs.push(("license_expression", Value::text(license.clone())));
		}
		if let Some(rights) = &self.rights {
			pairs.push(("rights", rights.to_value()));
		}
		if let Some(digest) = &self.sbom_digest {
			pairs.push(("sbom_digest", Value::bytes(digest.clone())));
		}
		pairs.push((
			"minimum_verifier_version",
			Value::int(i64::from(self.minimum_verifier_version)),
		));
		pairs.push((
			"critical_extensions",
			Value::array(self.critical_extensions.iter().cloned().map(Value::text).collect::<Vec<_>>()),
		));
		map_of("ReleasePayload", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("ReleasePayload", value)?.reject_unknown(&[
			"protocol",
			"type",
			"project_id",
			"game_id",
			"release_nonce",
			"human_version",
			"channel",
			"kind",
			"declared_time",
			"compatibility",
			"artifacts",
			"dependencies",
			"source_reference",
			"changelog_digest",
			"license_expression",
			"rights",
			"sbom_digest",
			"minimum_verifier_version",
			"critical_extensions",
		])?;
		expect_type(&fields, "release")?;
		let release = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			game_id: expect_text(fields.required("game_id")?, "game_id")?,
			release_nonce: expect_bytes(fields.required("release_nonce")?, "release_nonce")?,
			human_version: expect_text(fields.required("human_version")?, "human_version")?,
			channel: expect_text(fields.required("channel")?, "channel")?,
			kind: expect_text(fields.required("kind")?, "kind")?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
			compatibility: expect_array(fields.required("compatibility")?, "compatibility")?
				.iter()
				.cloned()
				.map(Compatibility::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			artifacts: expect_array(fields.required("artifacts")?, "artifacts")?
				.iter()
				.cloned()
				.map(Artifact::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			dependencies: expect_array(fields.required("dependencies")?, "dependencies")?
				.iter()
				.cloned()
				.map(Dependency::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			source_reference: fields
				.optional("source_reference")
				.map(|v| expect_text(v, "source_reference"))
				.transpose()?,
			changelog_digest: fields
				.optional("changelog_digest")
				.map(|v| expect_bytes(v, "changelog_digest"))
				.transpose()?,
			license_expression: fields
				.optional("license_expression")
				.map(|v| expect_text(v, "license_expression"))
				.transpose()?,
			rights: fields.optional("rights").cloned().map(Rights::from_value).transpose()?,
			sbom_digest: fields
				.optional("sbom_digest")
				.map(|v| expect_bytes(v, "sbom_digest"))
				.transpose()?,
			minimum_verifier_version: expect_u32(fields.required("minimum_verifier_version")?, "minimum_verifier_version")?,
			critical_extensions: crate::canonical::expect_text_array(
				fields.required("critical_extensions")?,
				"critical_extensions",
			)?,
		};
		release.validate()?;
		Ok(release)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Withdrawal {
	pub protocol: u32,
	pub project_id: String,
	pub release_id: String,
	pub reason: String,
	pub note: Option<String>,
	pub declared_time: i64,
}

impl Withdrawal {
	pub const REASONS: &'static [&'static str] = &["compromise", "harmful", "broken", "legal", "author-preference"];

	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.release_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "release_id"));
		}
		if self.project_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "project_id"));
		}
		if !Self::REASONS.contains(&self.reason.as_str()) {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "reason"));
		}
		Ok(())
	}
}

impl Canonical for Withdrawal {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("type", Value::text("withdrawal")),
			("project_id", Value::text(self.project_id.clone())),
			("release_id", Value::text(self.release_id.clone())),
			("reason", Value::text(self.reason.clone())),
		];
		if let Some(note) = &self.note {
			pairs.push(("note", Value::text(note.clone())));
		}
		pairs.push(("declared_time", Value::int(self.declared_time)));
		map_of("Withdrawal", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Withdrawal", value)?.reject_unknown(&[
			"protocol",
			"type",
			"project_id",
			"release_id",
			"reason",
			"note",
			"declared_time",
		])?;
		expect_type(&fields, "withdrawal")?;
		let withdrawal = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			release_id: expect_text(fields.required("release_id")?, "release_id")?,
			reason: expect_text(fields.required("reason")?, "reason")?,
			note: fields.optional("note").map(|v| expect_text(v, "note")).transpose()?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		withdrawal.validate()?;
		Ok(withdrawal)
	}
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseObject {
	Release(ReleasePayload),
	Location(LocationRecord),
	Withdrawal(Withdrawal),
}

impl ReleaseObject {
	pub fn validate(&self) -> Result<(), ModelError> {
		match self {
			Self::Release(payload) => payload.validate(),
			Self::Location(location) => location.validate(),
			Self::Withdrawal(withdrawal) => withdrawal.validate(),
		}
	}
}

impl Canonical for ReleaseObject {
	fn to_value(&self) -> Value {
		match self {
			Self::Release(release) => release.to_value(),
			Self::Location(location) => location.to_value(),
			Self::Withdrawal(withdrawal) => withdrawal.to_value(),
		}
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let discriminant = value
			.get("type")
			.and_then(Value::as_text)
			.ok_or_else(|| ModelError::field(RejectReason::MissingField, "type"))?;
		match discriminant {
			"release" => Ok(Self::Release(ReleasePayload::from_value(value)?)),
			"location" => Ok(Self::Location(LocationRecord::from_value(value)?)),
			"withdrawal" => Ok(Self::Withdrawal(Withdrawal::from_value(value)?)),
			_ => Err(ModelError::field(RejectReason::InvalidFieldValue, "type")),
		}
	}
}

fn expect_type(fields: &Fields, expected: &str) -> Result<(), ModelError> {
	let found = expect_text(fields.required("type")?, "type")?;
	if found != expected {
		return Err(ModelError::field(RejectReason::InvalidFieldValue, "type"));
	}
	Ok(())
}
