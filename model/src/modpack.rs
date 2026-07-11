use moraine_codec::Value;

use crate::canonical::{Canonical, Fields, expect_array, expect_bytes, expect_i64, expect_text, expect_u32, map_of};
use crate::compatibility::Side;
use crate::dependency::TargetKind;
use crate::error::{ModelError, RejectReason};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModpackEntry {
	pub ordinal: u32,
	pub target_kind: TargetKind,
	pub target_id: String,
	pub release_id: String,
	pub digest: Vec<u8>,
	pub applies_to: Side,
}

impl Canonical for ModpackEntry {
	fn to_value(&self) -> Value {
		map_of(
			"ModpackEntry",
			[
				("ordinal", Value::int(i64::from(self.ordinal))),
				("target_kind", Value::text(self.target_kind.as_str())),
				("target_id", Value::text(self.target_id.clone())),
				("release_id", Value::text(self.release_id.clone())),
				("digest", Value::bytes(self.digest.clone())),
				("applies_to", Value::text(self.applies_to.as_str())),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("ModpackEntry", value)?.reject_unknown(&[
			"ordinal",
			"target_kind",
			"target_id",
			"release_id",
			"digest",
			"applies_to",
		])?;
		let kind_text = expect_text(fields.required("target_kind")?, "target_kind")?;
		let side_text = expect_text(fields.required("applies_to")?, "applies_to")?;
		let digest = expect_bytes(fields.required("digest")?, "digest")?;
		if digest.len() != 32 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "digest"));
		}
		Ok(Self {
			ordinal: expect_u32(fields.required("ordinal")?, "ordinal")?,
			target_kind: TargetKind::parse(&kind_text)
				.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "target_kind"))?,
			target_id: expect_text(fields.required("target_id")?, "target_id")?,
			release_id: expect_text(fields.required("release_id")?, "release_id")?,
			digest,
			applies_to: Side::parse(&side_text)
				.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "applies_to"))?,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModpackOverride {
	pub digest: Vec<u8>,
	pub target_path: String,
	pub applies_to: Side,
}

impl Canonical for ModpackOverride {
	fn to_value(&self) -> Value {
		map_of(
			"ModpackOverride",
			[
				("digest", Value::bytes(self.digest.clone())),
				("target_path", Value::text(self.target_path.clone())),
				("applies_to", Value::text(self.applies_to.as_str())),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("ModpackOverride", value)?.reject_unknown(&["digest", "target_path", "applies_to"])?;
		let side_text = expect_text(fields.required("applies_to")?, "applies_to")?;
		let override_file = Self {
			digest: expect_bytes(fields.required("digest")?, "digest")?,
			target_path: expect_text(fields.required("target_path")?, "target_path")?,
			applies_to: Side::parse(&side_text)
				.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "applies_to"))?,
		};
		if override_file.digest.len() != 32 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "digest"));
		}
		if !valid_override_path(&override_file.target_path) {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "target_path"));
		}
		Ok(override_file)
	}
}

/// An override path is relative and never escapes its root. Absolute paths and
/// `..` segments are rejected before an adapter ever sees them.
pub fn valid_override_path(path: &str) -> bool {
	!path.is_empty()
		&& !path.starts_with('/')
		&& !path.starts_with('\\')
		&& !path.contains(':')
		&& path
			.split('/')
			.all(|segment| !segment.is_empty() && segment != ".." && segment != ".")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModpackManifest {
	pub protocol: u32,
	pub project_id: String,
	pub game_id: String,
	pub loader_id: Option<String>,
	pub entries: Vec<ModpackEntry>,
	pub overrides: Vec<ModpackOverride>,
	pub server_manifest_digest: Option<Vec<u8>>,
	pub declared_time: i64,
}

impl ModpackManifest {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.project_id.is_empty() || self.game_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "project_id"));
		}
		if self.entries.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "entries"));
		}
		let mut ordinals = std::collections::HashSet::new();
		for entry in &self.entries {
			if !ordinals.insert(entry.ordinal) {
				return Err(ModelError::field(RejectReason::InvalidFieldValue, "ordinal"));
			}
		}
		if let Some(digest) = &self.server_manifest_digest
			&& digest.len() != 32
		{
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "server_manifest_digest"));
		}
		Ok(())
	}
}

impl Canonical for ModpackManifest {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("project_id", Value::text(self.project_id.clone())),
			("game_id", Value::text(self.game_id.clone())),
		];
		if let Some(loader_id) = &self.loader_id {
			pairs.push(("loader_id", Value::text(loader_id.clone())));
		}
		pairs.push((
			"entries",
			Value::array(self.entries.iter().map(Canonical::to_value).collect::<Vec<_>>()),
		));
		pairs.push((
			"overrides",
			Value::array(self.overrides.iter().map(Canonical::to_value).collect::<Vec<_>>()),
		));
		if let Some(digest) = &self.server_manifest_digest {
			pairs.push(("server_manifest_digest", Value::bytes(digest.clone())));
		}
		pairs.push(("declared_time", Value::int(self.declared_time)));
		map_of("ModpackManifest", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("ModpackManifest", value)?.reject_unknown(&[
			"protocol",
			"project_id",
			"game_id",
			"loader_id",
			"entries",
			"overrides",
			"server_manifest_digest",
			"declared_time",
		])?;
		let manifest = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			game_id: expect_text(fields.required("game_id")?, "game_id")?,
			loader_id: fields
				.optional("loader_id")
				.map(|value| expect_text(value, "loader_id"))
				.transpose()?,
			entries: expect_array(fields.required("entries")?, "entries")?
				.iter()
				.cloned()
				.map(ModpackEntry::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			overrides: expect_array(fields.required("overrides")?, "overrides")?
				.iter()
				.cloned()
				.map(ModpackOverride::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			server_manifest_digest: fields
				.optional("server_manifest_digest")
				.map(|value| expect_bytes(value, "server_manifest_digest"))
				.transpose()?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		manifest.validate()?;
		Ok(manifest)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn override_paths_cannot_escape_their_root() {
		assert!(valid_override_path("config/example.toml"));
		assert!(!valid_override_path("/etc/passwd"));
		assert!(!valid_override_path("../outside.txt"));
		assert!(!valid_override_path("a/../../b"));
		assert!(!valid_override_path(""));
		assert!(!valid_override_path("C:/windows"));
	}
}
