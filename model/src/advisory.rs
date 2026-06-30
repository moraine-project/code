use moraine_codec::Value;

use crate::canonical::{Canonical, Fields, expect_bool, expect_bytes, expect_i64, expect_text, expect_u32, map_of};
use crate::compatibility::Predicate;
use crate::error::{ModelError, RejectReason};

pub const TAXONOMY_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
	Info,
	Low,
	Moderate,
	High,
	Critical,
}

impl Severity {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Info => "info",
			Self::Low => "low",
			Self::Moderate => "moderate",
			Self::High => "high",
			Self::Critical => "critical",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"info" => Self::Info,
			"low" => Self::Low,
			"moderate" => Self::Moderate,
			"high" => Self::High,
			"critical" => Self::Critical,
			_ => return None,
		})
	}

	const fn is_high_or_critical(self) -> bool {
		matches!(self, Self::High | Self::Critical)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
	Malware,
	Vulnerability,
	KnownIncompatibility,
	Privacy,
	Policy,
	Other,
}

impl Category {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Malware => "malware",
			Self::Vulnerability => "vulnerability",
			Self::KnownIncompatibility => "known-incompatibility",
			Self::Privacy => "privacy",
			Self::Policy => "policy",
			Self::Other => "other",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"malware" => Self::Malware,
			"vulnerability" => Self::Vulnerability,
			"known-incompatibility" => Self::KnownIncompatibility,
			"privacy" => Self::Privacy,
			"policy" => Self::Policy,
			"other" => Self::Other,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taxonomy {
	Known,
	Unrecognized,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Affected {
	pub digest: Option<Vec<u8>>,
	pub predicate: Option<Predicate>,
}

impl Canonical for Affected {
	fn to_value(&self) -> Value {
		let mut pairs = Vec::new();
		if let Some(digest) = &self.digest {
			pairs.push(("digest", Value::bytes(digest.clone())));
		}
		if let Some(predicate) = &self.predicate {
			pairs.push(("predicate", predicate.to_value()));
		}
		map_of("Affected", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Affected", value)?.reject_unknown(&["digest", "predicate"])?;
		let affected = Self {
			digest: fields.optional("digest").map(|v| expect_bytes(v, "digest")).transpose()?,
			predicate: fields.optional("predicate").cloned().map(Predicate::from_value).transpose()?,
		};
		if affected.digest.is_none() && affected.predicate.is_none() {
			return Err(ModelError::field(RejectReason::MissingField, "affected"));
		}
		if let Some(digest) = &affected.digest
			&& digest.len() != 32
		{
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "digest"));
		}
		Ok(affected)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advisory {
	pub protocol: u32,
	pub provider_id: String,
	pub project_id: String,
	pub game_id: String,
	pub affected: Affected,
	pub severity: Severity,
	pub category: Category,
	pub taxonomy_version: u32,
	pub block_promotion: bool,
	pub evidence_ref: Option<String>,
	pub published_at: i64,
	pub expires_at: Option<i64>,
	pub retracted_at: Option<i64>,
}

impl Advisory {
	pub fn taxonomy(&self) -> Taxonomy {
		if self.taxonomy_version == TAXONOMY_VERSION {
			Taxonomy::Known
		} else {
			Taxonomy::Unrecognized
		}
	}

	pub fn blocks_promotion(&self) -> Result<bool, ModelError> {
		if self.taxonomy() == Taxonomy::Unrecognized {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"unrecognized advisory taxonomy version",
			));
		}
		Ok(self.block_promotion)
	}

	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.provider_id.is_empty() || self.project_id.is_empty() || self.game_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "provider_id"));
		}
		if self.taxonomy_version == 0 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "taxonomy_version"));
		}
		if self.block_promotion && !(self.category == Category::Malware && self.severity.is_high_or_critical()) {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"block_promotion is only valid for malware at high or critical severity",
			));
		}
		if let (Some(retracted), Some(expires)) = (self.retracted_at, self.expires_at)
			&& retracted > expires
		{
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "retracted_at"));
		}
		Ok(())
	}
}

impl Canonical for Advisory {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("provider_id", Value::text(self.provider_id.clone())),
			("project_id", Value::text(self.project_id.clone())),
			("game_id", Value::text(self.game_id.clone())),
			("affected", self.affected.to_value()),
			("severity", Value::text(self.severity.as_str())),
			("category", Value::text(self.category.as_str())),
			("taxonomy_version", Value::int(i64::from(self.taxonomy_version))),
			("block_promotion", Value::Bool(self.block_promotion)),
		];
		if let Some(evidence_ref) = &self.evidence_ref {
			pairs.push(("evidence_ref", Value::text(evidence_ref.clone())));
		}
		pairs.push(("published_at", Value::int(self.published_at)));
		if let Some(expires_at) = self.expires_at {
			pairs.push(("expires_at", Value::int(expires_at)));
		}
		if let Some(retracted_at) = self.retracted_at {
			pairs.push(("retracted_at", Value::int(retracted_at)));
		}
		map_of("Advisory", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Advisory", value)?.reject_unknown(&[
			"protocol",
			"provider_id",
			"project_id",
			"game_id",
			"affected",
			"severity",
			"category",
			"taxonomy_version",
			"block_promotion",
			"evidence_ref",
			"published_at",
			"expires_at",
			"retracted_at",
		])?;
		let severity_text = expect_text(fields.required("severity")?, "severity")?;
		let category_text = expect_text(fields.required("category")?, "category")?;
		let advisory = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			provider_id: expect_text(fields.required("provider_id")?, "provider_id")?,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			game_id: expect_text(fields.required("game_id")?, "game_id")?,
			affected: Affected::from_value(fields.required("affected")?.clone())?,
			severity: Severity::parse(&severity_text)
				.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "severity"))?,
			category: Category::parse(&category_text)
				.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "category"))?,
			taxonomy_version: expect_u32(fields.required("taxonomy_version")?, "taxonomy_version")?,
			block_promotion: expect_bool(fields.required("block_promotion")?, "block_promotion")?,
			evidence_ref: fields
				.optional("evidence_ref")
				.map(|v| expect_text(v, "evidence_ref"))
				.transpose()?,
			published_at: expect_i64(fields.required("published_at")?, "published_at")?,
			expires_at: fields
				.optional("expires_at")
				.map(|v| expect_i64(v, "expires_at"))
				.transpose()?,
			retracted_at: fields
				.optional("retracted_at")
				.map(|v| expect_i64(v, "retracted_at"))
				.transpose()?,
		};
		advisory.validate()?;
		Ok(advisory)
	}
}
