use moraine_codec::Value;

use crate::canonical::{Canonical, Fields, expect_text, map_of};
use crate::compatibility::{Predicate, Side};
use crate::error::{ModelError, RejectReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
	Project,
	Loader,
	Runtime,
}

impl TargetKind {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Project => "project",
			Self::Loader => "loader",
			Self::Runtime => "runtime",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"project" => Self::Project,
			"loader" => Self::Loader,
			"runtime" => Self::Runtime,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
	Required,
	Optional,
	Embedded,
	Incompatible,
	Recommended,
}

impl DependencyKind {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Required => "required",
			Self::Optional => "optional",
			Self::Embedded => "embedded",
			Self::Incompatible => "incompatible",
			Self::Recommended => "recommended",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"required" => Self::Required,
			"optional" => Self::Optional,
			"embedded" => Self::Embedded,
			"incompatible" => Self::Incompatible,
			"recommended" => Self::Recommended,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
	pub target_kind: TargetKind,
	pub target_id: String,
	pub game_id: String,
	pub predicate: Predicate,
	pub kind: DependencyKind,
	pub applies_to: Side,
}

impl Canonical for Dependency {
	fn to_value(&self) -> Value {
		map_of(
			"Dependency",
			[
				("target_kind", Value::text(self.target_kind.as_str())),
				("target_id", Value::text(self.target_id.clone())),
				("game_id", Value::text(self.game_id.clone())),
				("predicate", self.predicate.to_value()),
				("kind", Value::text(self.kind.as_str())),
				("applies_to", Value::text(self.applies_to.as_str())),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Dependency", value)?.reject_unknown(&[
			"target_kind",
			"target_id",
			"game_id",
			"predicate",
			"kind",
			"applies_to",
		])?;
		let target_text = expect_text(fields.required("target_kind")?, "target_kind")?;
		let target_kind = TargetKind::parse(&target_text)
			.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "target_kind"))?;
		let kind_text = expect_text(fields.required("kind")?, "kind")?;
		let kind =
			DependencyKind::parse(&kind_text).ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "kind"))?;
		let side_text = expect_text(fields.required("applies_to")?, "applies_to")?;
		let applies_to =
			Side::parse(&side_text).ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "applies_to"))?;
		Ok(Self {
			target_kind,
			target_id: expect_text(fields.required("target_id")?, "target_id")?,
			game_id: expect_text(fields.required("game_id")?, "game_id")?,
			predicate: Predicate::from_value(fields.required("predicate")?.clone())?,
			kind,
			applies_to,
		})
	}
}
