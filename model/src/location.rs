use moraine_codec::Value;

use crate::canonical::{Canonical, Fields, expect_array, expect_bytes, expect_i64, expect_text, expect_u32, map_of};
use crate::error::{ModelError, RejectReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationKind {
	Origin,
	Mirror,
	External,
}

impl LocationKind {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Origin => "origin",
			Self::Mirror => "mirror",
			Self::External => "external",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"origin" => Self::Origin,
			"mirror" => Self::Mirror,
			"external" => Self::External,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
	pub url: String,
	pub kind: LocationKind,
	pub operator_id: Option<String>,
}

impl Canonical for Location {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("url", Value::text(self.url.clone())),
			("kind", Value::text(self.kind.as_str())),
		];
		if let Some(operator_id) = &self.operator_id {
			pairs.push(("operator_id", Value::text(operator_id.clone())));
		}
		map_of("Location", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Location", value)?.reject_unknown(&["url", "kind", "operator_id"])?;
		let kind_text = expect_text(fields.required("kind")?, "kind")?;
		let location = Self {
			url: expect_text(fields.required("url")?, "url")?,
			kind: LocationKind::parse(&kind_text)
				.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "kind"))?,
			operator_id: fields
				.optional("operator_id")
				.map(|v| expect_text(v, "operator_id"))
				.transpose()?,
		};
		if location.url.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "url"));
		}
		Ok(location)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationRecord {
	pub protocol: u32,
	pub artifact_digest: Vec<u8>,
	pub locations: Vec<Location>,
	pub declared_time: i64,
}

impl LocationRecord {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.artifact_digest.len() != 32 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "artifact_digest"));
		}
		if self.locations.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "locations"));
		}
		Ok(())
	}
}

impl Canonical for LocationRecord {
	fn to_value(&self) -> Value {
		map_of(
			"LocationRecord",
			[
				("protocol", Value::int(i64::from(self.protocol))),
				("type", Value::text("location")),
				("artifact_digest", Value::bytes(self.artifact_digest.clone())),
				(
					"locations",
					Value::array(self.locations.iter().map(Canonical::to_value).collect::<Vec<_>>()),
				),
				("declared_time", Value::int(self.declared_time)),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("LocationRecord", value)?.reject_unknown(&[
			"protocol",
			"type",
			"artifact_digest",
			"locations",
			"declared_time",
		])?;
		expect_type(&fields, "location")?;
		let record = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			artifact_digest: expect_bytes(fields.required("artifact_digest")?, "artifact_digest")?,
			locations: expect_array(fields.required("locations")?, "locations")?
				.iter()
				.cloned()
				.map(Location::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		record.validate()?;
		Ok(record)
	}
}

fn expect_type(fields: &Fields, expected: &str) -> Result<(), ModelError> {
	let found = expect_text(fields.required("type")?, "type")?;
	if found != expected {
		return Err(ModelError::field(RejectReason::InvalidFieldValue, "type"));
	}
	Ok(())
}
