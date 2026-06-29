use moraine_codec::Value;

use crate::canonical::{Canonical, Fields, expect_bytes, expect_i64, expect_text, expect_u32, map_of};
use crate::error::{ModelError, RejectReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttestationKind {
	BuildProvenance,
	Review,
	ScannerResult,
	Sbom,
	CompatibilityTest,
}

impl AttestationKind {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::BuildProvenance => "build-provenance",
			Self::Review => "review",
			Self::ScannerResult => "scanner-result",
			Self::Sbom => "sbom",
			Self::CompatibilityTest => "compatibility-test",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"build-provenance" => Self::BuildProvenance,
			"review" => Self::Review,
			"scanner-result" => Self::ScannerResult,
			"sbom" => Self::Sbom,
			"compatibility-test" => Self::CompatibilityTest,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
	pub protocol: u32,
	pub artifact_digest: Vec<u8>,
	pub subject_kind: String,
	pub subject_id: String,
	pub kind: AttestationKind,
	pub media_type: String,
	pub body_digest: Option<Vec<u8>>,
	pub body_inline: Option<Vec<u8>>,
	pub signer_id: String,
	pub issued_at: i64,
}

impl Attestation {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.artifact_digest.len() != 32 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "artifact_digest"));
		}
		if !matches!(self.subject_kind.as_str(), "project" | "release" | "loader-release") {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "subject_kind"));
		}
		if self.signer_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "signer_id"));
		}
		if self.body_digest.is_some() && self.body_inline.is_some() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "body_digest"));
		}
		if let Some(digest) = &self.body_digest
			&& digest.len() != 32
		{
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "body_digest"));
		}
		Ok(())
	}
}

impl Canonical for Attestation {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("type", Value::text("attestation")),
			("artifact_digest", Value::bytes(self.artifact_digest.clone())),
			("subject_kind", Value::text(self.subject_kind.clone())),
			("subject_id", Value::text(self.subject_id.clone())),
			("kind", Value::text(self.kind.as_str())),
			("media_type", Value::text(self.media_type.clone())),
		];
		if let Some(digest) = &self.body_digest {
			pairs.push(("body_digest", Value::bytes(digest.clone())));
		}
		if let Some(inline) = &self.body_inline {
			pairs.push(("body_inline", Value::bytes(inline.clone())));
		}
		pairs.push(("signer_id", Value::text(self.signer_id.clone())));
		pairs.push(("issued_at", Value::int(self.issued_at)));
		map_of("Attestation", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Attestation", value)?.reject_unknown(&[
			"protocol",
			"type",
			"artifact_digest",
			"subject_kind",
			"subject_id",
			"kind",
			"media_type",
			"body_digest",
			"body_inline",
			"signer_id",
			"issued_at",
		])?;
		expect_type(&fields, "attestation")?;
		let kind_text = expect_text(fields.required("kind")?, "kind")?;
		let attestation = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			artifact_digest: expect_bytes(fields.required("artifact_digest")?, "artifact_digest")?,
			subject_kind: expect_text(fields.required("subject_kind")?, "subject_kind")?,
			subject_id: expect_text(fields.required("subject_id")?, "subject_id")?,
			kind: AttestationKind::parse(&kind_text)
				.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "kind"))?,
			media_type: expect_text(fields.required("media_type")?, "media_type")?,
			body_digest: fields
				.optional("body_digest")
				.map(|v| expect_bytes(v, "body_digest"))
				.transpose()?,
			body_inline: fields
				.optional("body_inline")
				.map(|v| expect_bytes(v, "body_inline"))
				.transpose()?,
			signer_id: expect_text(fields.required("signer_id")?, "signer_id")?,
			issued_at: expect_i64(fields.required("issued_at")?, "issued_at")?,
		};
		attestation.validate()?;
		Ok(attestation)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorCommitment {
	pub protocol: u32,
	pub mirror_id: String,
	pub artifact_digest: Vec<u8>,
	pub size: u64,
	pub accepted_at: i64,
	pub retention_until: Option<i64>,
	pub endpoint: String,
}

impl MirrorCommitment {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.mirror_id.is_empty() || self.endpoint.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "mirror_id"));
		}
		if self.artifact_digest.len() != 32 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "artifact_digest"));
		}
		Ok(())
	}
}

impl Canonical for MirrorCommitment {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("type", Value::text("mirror-commitment")),
			("mirror_id", Value::text(self.mirror_id.clone())),
			("artifact_digest", Value::bytes(self.artifact_digest.clone())),
			("size", Value::int(self.size as i64)),
			("accepted_at", Value::int(self.accepted_at)),
		];
		if let Some(retention_until) = self.retention_until {
			pairs.push(("retention_until", Value::int(retention_until)));
		}
		pairs.push(("endpoint", Value::text(self.endpoint.clone())));
		map_of("MirrorCommitment", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("MirrorCommitment", value)?.reject_unknown(&[
			"protocol",
			"type",
			"mirror_id",
			"artifact_digest",
			"size",
			"accepted_at",
			"retention_until",
			"endpoint",
		])?;
		expect_type(&fields, "mirror-commitment")?;
		let commitment = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			mirror_id: expect_text(fields.required("mirror_id")?, "mirror_id")?,
			artifact_digest: expect_bytes(fields.required("artifact_digest")?, "artifact_digest")?,
			size: u64::try_from(expect_i64(fields.required("size")?, "size")?)
				.map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, "size"))?,
			accepted_at: expect_i64(fields.required("accepted_at")?, "accepted_at")?,
			retention_until: fields
				.optional("retention_until")
				.map(|v| expect_i64(v, "retention_until"))
				.transpose()?,
			endpoint: expect_text(fields.required("endpoint")?, "endpoint")?,
		};
		commitment.validate()?;
		Ok(commitment)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttestationObject {
	Evidence(Attestation),
	MirrorCommitment(MirrorCommitment),
}

impl Canonical for AttestationObject {
	fn to_value(&self) -> Value {
		match self {
			Self::Evidence(attestation) => attestation.to_value(),
			Self::MirrorCommitment(commitment) => commitment.to_value(),
		}
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let discriminant = value
			.get("type")
			.and_then(Value::as_text)
			.ok_or_else(|| ModelError::field(RejectReason::MissingField, "type"))?;
		match discriminant {
			"attestation" => Ok(Self::Evidence(Attestation::from_value(value)?)),
			"mirror-commitment" => Ok(Self::MirrorCommitment(MirrorCommitment::from_value(value)?)),
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
