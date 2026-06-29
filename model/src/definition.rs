use moraine_codec::Value;

use crate::canonical::{
	Canonical, Fields, expect_array, expect_bool, expect_bytes, expect_i64, expect_text, expect_text_array, expect_u32,
	map_of,
};
use crate::compatibility::Predicate;
use crate::error::{ModelError, RejectReason};
use crate::reference::ArtifactRef;
use crate::version::OrderingScheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
	Java,
	Dotnet,
	Node,
	Native,
	Other,
}

impl RuntimeKind {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Java => "java",
			Self::Dotnet => "dotnet",
			Self::Node => "node",
			Self::Native => "native",
			Self::Other => "other",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"java" => Self::Java,
			"dotnet" => Self::Dotnet,
			"node" => Self::Node,
			"native" => Self::Native,
			"other" => Self::Other,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qualification {
	Native,
	Most,
	Experimental,
	Untested,
}

impl Qualification {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Native => "native",
			Self::Most => "most",
			Self::Experimental => "experimental",
			Self::Untested => "untested",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"native" => Self::Native,
			"most" => Self::Most,
			"experimental" => Self::Experimental,
			"untested" => Self::Untested,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionSyntax {
	pub kind: String,
	pub pattern: Option<String>,
}

impl Canonical for VersionSyntax {
	fn to_value(&self) -> Value {
		let mut pairs = vec![("kind", Value::text(self.kind.clone()))];
		if let Some(pattern) = &self.pattern {
			pairs.push(("pattern", Value::text(pattern.clone())));
		}
		map_of("VersionSyntax", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("VersionSyntax", value)?.reject_unknown(&["kind", "pattern"])?;
		let kind = expect_text(fields.required("kind")?, "kind")?;
		if OrderingScheme::parse(&kind).is_none() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "kind"));
		}
		Ok(Self {
			kind,
			pattern: fields.optional("pattern").map(|v| expect_text(v, "pattern")).transpose()?,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
	pub id: String,
	pub label: String,
	pub parent: Option<String>,
}

impl Canonical for Category {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("id", Value::text(self.id.clone())),
			("label", Value::text(self.label.clone())),
		];
		if let Some(parent) = &self.parent {
			pairs.push(("parent", Value::text(parent.clone())));
		}
		map_of("Category", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Category", value)?.reject_unknown(&["id", "label", "parent"])?;
		let category = Self {
			id: expect_text(fields.required("id")?, "id")?,
			label: expect_text(fields.required("label")?, "label")?,
			parent: fields.optional("parent").map(|v| expect_text(v, "parent")).transpose()?,
		};
		if category.id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "id"));
		}
		Ok(category)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
	pub id: String,
	pub label: String,
}

impl Canonical for Tag {
	fn to_value(&self) -> Value {
		map_of(
			"Tag",
			[
				("id", Value::text(self.id.clone())),
				("label", Value::text(self.label.clone())),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Tag", value)?.reject_unknown(&["id", "label"])?;
		let tag = Self {
			id: expect_text(fields.required("id")?, "id")?,
			label: expect_text(fields.required("label")?, "label")?,
		};
		if tag.id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "id"));
		}
		Ok(tag)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameDef {
	pub protocol: u32,
	pub game_id: String,
	pub display_name: String,
	pub version_syntax: VersionSyntax,
	pub version_ordering: String,
	pub loaders_allowed: bool,
	pub loader_authorities: Vec<String>,
	pub categories: Vec<Category>,
	pub tags: Vec<Tag>,
	pub metadata_extractor: Option<String>,
	pub install_adapter: Option<String>,
	pub declared_time: i64,
}

impl GameDef {
	pub fn ordering(&self) -> Option<OrderingScheme> {
		OrderingScheme::parse(&self.version_ordering)
	}

	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.game_id.is_empty() || self.display_name.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "game_id"));
		}
		if self.ordering().is_none() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "version_ordering"));
		}
		reject_duplicate_ids(self.categories.iter().map(|category| category.id.as_str()), "categories")?;
		reject_duplicate_ids(self.tags.iter().map(|tag| tag.id.as_str()), "tags")?;
		Ok(())
	}
}

impl Canonical for GameDef {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("game_id", Value::text(self.game_id.clone())),
			("display_name", Value::text(self.display_name.clone())),
			("version_syntax", self.version_syntax.to_value()),
			("version_ordering", Value::text(self.version_ordering.clone())),
			("loaders_allowed", Value::Bool(self.loaders_allowed)),
			(
				"loader_authorities",
				Value::array(self.loader_authorities.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			),
			(
				"categories",
				Value::array(self.categories.iter().map(Canonical::to_value).collect::<Vec<_>>()),
			),
			(
				"tags",
				Value::array(self.tags.iter().map(Canonical::to_value).collect::<Vec<_>>()),
			),
		];
		if let Some(extractor) = &self.metadata_extractor {
			pairs.push(("metadata_extractor", Value::text(extractor.clone())));
		}
		if let Some(adapter) = &self.install_adapter {
			pairs.push(("install_adapter", Value::text(adapter.clone())));
		}
		pairs.push(("declared_time", Value::int(self.declared_time)));
		map_of("GameDef", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("GameDef", value)?.reject_unknown(&[
			"protocol",
			"game_id",
			"display_name",
			"version_syntax",
			"version_ordering",
			"loaders_allowed",
			"loader_authorities",
			"categories",
			"tags",
			"metadata_extractor",
			"install_adapter",
			"declared_time",
		])?;
		let def = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			game_id: expect_text(fields.required("game_id")?, "game_id")?,
			display_name: expect_text(fields.required("display_name")?, "display_name")?,
			version_syntax: VersionSyntax::from_value(fields.required("version_syntax")?.clone())?,
			version_ordering: expect_text(fields.required("version_ordering")?, "version_ordering")?,
			loaders_allowed: expect_bool(fields.required("loaders_allowed")?, "loaders_allowed")?,
			loader_authorities: expect_text_array(fields.required("loader_authorities")?, "loader_authorities")?,
			categories: expect_array(fields.required("categories")?, "categories")?
				.iter()
				.cloned()
				.map(Category::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			tags: expect_array(fields.required("tags")?, "tags")?
				.iter()
				.cloned()
				.map(Tag::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			metadata_extractor: fields
				.optional("metadata_extractor")
				.map(|v| expect_text(v, "metadata_extractor"))
				.transpose()?,
			install_adapter: fields
				.optional("install_adapter")
				.map(|v| expect_text(v, "install_adapter"))
				.transpose()?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		def.validate()?;
		Ok(def)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDef {
	pub protocol: u32,
	pub runtime_id: String,
	pub kind: String,
	pub display_name: String,
	pub version_ordering: String,
	pub declared_time: i64,
}

impl RuntimeDef {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.runtime_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "runtime_id"));
		}
		if RuntimeKind::parse(&self.kind).is_none() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "kind"));
		}
		if OrderingScheme::parse(&self.version_ordering).is_none() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "version_ordering"));
		}
		Ok(())
	}
}

impl Canonical for RuntimeDef {
	fn to_value(&self) -> Value {
		map_of(
			"RuntimeDef",
			[
				("protocol", Value::int(i64::from(self.protocol))),
				("runtime_id", Value::text(self.runtime_id.clone())),
				("kind", Value::text(self.kind.clone())),
				("display_name", Value::text(self.display_name.clone())),
				("version_ordering", Value::text(self.version_ordering.clone())),
				("declared_time", Value::int(self.declared_time)),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("RuntimeDef", value)?.reject_unknown(&[
			"protocol",
			"runtime_id",
			"kind",
			"display_name",
			"version_ordering",
			"declared_time",
		])?;
		let def = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			runtime_id: expect_text(fields.required("runtime_id")?, "runtime_id")?,
			kind: expect_text(fields.required("kind")?, "kind")?,
			display_name: expect_text(fields.required("display_name")?, "display_name")?,
			version_ordering: expect_text(fields.required("version_ordering")?, "version_ordering")?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		def.validate()?;
		Ok(def)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredBy {
	pub kind: String,
	pub id: String,
}

impl Canonical for DeclaredBy {
	fn to_value(&self) -> Value {
		map_of(
			"DeclaredBy",
			[("kind", Value::text(self.kind.clone())), ("id", Value::text(self.id.clone()))],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("DeclaredBy", value)?.reject_unknown(&["kind", "id"])?;
		let declared_by = Self {
			kind: expect_text(fields.required("kind")?, "kind")?,
			id: expect_text(fields.required("id")?, "id")?,
		};
		if !matches!(declared_by.kind.as_str(), "loader-authority" | "project" | "tester") || declared_by.id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "kind"));
		}
		Ok(declared_by)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderDef {
	pub protocol: u32,
	pub loader_id: String,
	pub game_id: String,
	pub display_name: String,
	pub version_ordering: String,
	pub bootstrap: Option<ArtifactRef>,
	pub accepted_artifacts: Option<Vec<String>>,
	pub declared_time: i64,
}

impl LoaderDef {
	pub fn ordering(&self) -> Option<OrderingScheme> {
		OrderingScheme::parse(&self.version_ordering)
	}

	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.loader_id.is_empty() || self.game_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "loader_id"));
		}
		if self.ordering().is_none() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "version_ordering"));
		}
		Ok(())
	}
}

impl Canonical for LoaderDef {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("type", Value::text("definition")),
			("loader_id", Value::text(self.loader_id.clone())),
			("game_id", Value::text(self.game_id.clone())),
			("display_name", Value::text(self.display_name.clone())),
			("version_ordering", Value::text(self.version_ordering.clone())),
		];
		if let Some(bootstrap) = &self.bootstrap {
			pairs.push(("bootstrap", bootstrap.to_value()));
		}
		if let Some(accepted) = &self.accepted_artifacts {
			pairs.push((
				"accepted_artifacts",
				Value::array(accepted.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			));
		}
		pairs.push(("declared_time", Value::int(self.declared_time)));
		map_of("LoaderDef", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("LoaderDef", value)?.reject_unknown(&[
			"protocol",
			"type",
			"loader_id",
			"game_id",
			"display_name",
			"version_ordering",
			"bootstrap",
			"accepted_artifacts",
			"declared_time",
		])?;
		expect_type(&fields, "definition")?;
		let def = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			loader_id: expect_text(fields.required("loader_id")?, "loader_id")?,
			game_id: expect_text(fields.required("game_id")?, "game_id")?,
			display_name: expect_text(fields.required("display_name")?, "display_name")?,
			version_ordering: expect_text(fields.required("version_ordering")?, "version_ordering")?,
			bootstrap: fields
				.optional("bootstrap")
				.cloned()
				.map(ArtifactRef::from_value)
				.transpose()?,
			accepted_artifacts: fields
				.optional("accepted_artifacts")
				.map(|v| expect_text_array(v, "accepted_artifacts"))
				.transpose()?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		def.validate()?;
		Ok(def)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderRelease {
	pub protocol: u32,
	pub loader_id: String,
	pub version_id: String,
	pub game_version_predicate: Predicate,
	pub runtime_id: Option<String>,
	pub runtime_predicate: Option<Predicate>,
	pub bootstrap: Option<ArtifactRef>,
	pub declared_time: i64,
}

impl LoaderRelease {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.loader_id.is_empty() || self.version_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "version_id"));
		}
		Ok(())
	}
}

impl Canonical for LoaderRelease {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("type", Value::text("release")),
			("loader_id", Value::text(self.loader_id.clone())),
			("version_id", Value::text(self.version_id.clone())),
			("game_version_predicate", self.game_version_predicate.to_value()),
		];
		if let Some(runtime_id) = &self.runtime_id {
			pairs.push(("runtime_id", Value::text(runtime_id.clone())));
		}
		if let Some(predicate) = &self.runtime_predicate {
			pairs.push(("runtime_predicate", predicate.to_value()));
		}
		if let Some(bootstrap) = &self.bootstrap {
			pairs.push(("bootstrap", bootstrap.to_value()));
		}
		pairs.push(("declared_time", Value::int(self.declared_time)));
		map_of("LoaderRelease", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("LoaderRelease", value)?.reject_unknown(&[
			"protocol",
			"type",
			"loader_id",
			"version_id",
			"game_version_predicate",
			"runtime_id",
			"runtime_predicate",
			"bootstrap",
			"declared_time",
		])?;
		expect_type(&fields, "release")?;
		let release = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			loader_id: expect_text(fields.required("loader_id")?, "loader_id")?,
			version_id: expect_text(fields.required("version_id")?, "version_id")?,
			game_version_predicate: Predicate::from_value(fields.required("game_version_predicate")?.clone())?,
			runtime_id: fields
				.optional("runtime_id")
				.map(|v| expect_text(v, "runtime_id"))
				.transpose()?,
			runtime_predicate: fields
				.optional("runtime_predicate")
				.cloned()
				.map(Predicate::from_value)
				.transpose()?,
			bootstrap: fields
				.optional("bootstrap")
				.cloned()
				.map(ArtifactRef::from_value)
				.transpose()?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		release.validate()?;
		Ok(release)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderAcceptance {
	pub protocol: u32,
	pub accepting_loader_id: String,
	pub accepted_loader_id: String,
	pub game_id: String,
	pub game_version_predicate: Option<Predicate>,
	pub loader_version_predicate: Option<Predicate>,
	pub accepted_version_predicate: Option<Predicate>,
	pub qualification: Qualification,
	pub declared_by: DeclaredBy,
	pub evidence_digest: Option<Vec<u8>>,
	pub declared_time: i64,
}

impl LoaderAcceptance {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.accepting_loader_id.is_empty() || self.accepted_loader_id.is_empty() || self.game_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "accepting_loader_id"));
		}
		if let Some(digest) = &self.evidence_digest
			&& digest.len() != 32
		{
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "evidence_digest"));
		}
		Ok(())
	}
}

impl Canonical for LoaderAcceptance {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("type", Value::text("mapping")),
			("accepting_loader_id", Value::text(self.accepting_loader_id.clone())),
			("accepted_loader_id", Value::text(self.accepted_loader_id.clone())),
			("game_id", Value::text(self.game_id.clone())),
		];
		if let Some(predicate) = &self.game_version_predicate {
			pairs.push(("game_version_predicate", predicate.to_value()));
		}
		if let Some(predicate) = &self.loader_version_predicate {
			pairs.push(("loader_version_predicate", predicate.to_value()));
		}
		if let Some(predicate) = &self.accepted_version_predicate {
			pairs.push(("accepted_version_predicate", predicate.to_value()));
		}
		pairs.push(("qualification", Value::text(self.qualification.as_str())));
		pairs.push(("declared_by", self.declared_by.to_value()));
		if let Some(digest) = &self.evidence_digest {
			pairs.push(("evidence_digest", Value::bytes(digest.clone())));
		}
		pairs.push(("declared_time", Value::int(self.declared_time)));
		map_of("LoaderAcceptance", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("LoaderAcceptance", value)?.reject_unknown(&[
			"protocol",
			"type",
			"accepting_loader_id",
			"accepted_loader_id",
			"game_id",
			"game_version_predicate",
			"loader_version_predicate",
			"accepted_version_predicate",
			"qualification",
			"declared_by",
			"evidence_digest",
			"declared_time",
		])?;
		expect_type(&fields, "mapping")?;
		let qualification_text = expect_text(fields.required("qualification")?, "qualification")?;
		let acceptance = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			accepting_loader_id: expect_text(fields.required("accepting_loader_id")?, "accepting_loader_id")?,
			accepted_loader_id: expect_text(fields.required("accepted_loader_id")?, "accepted_loader_id")?,
			game_id: expect_text(fields.required("game_id")?, "game_id")?,
			game_version_predicate: fields
				.optional("game_version_predicate")
				.cloned()
				.map(Predicate::from_value)
				.transpose()?,
			loader_version_predicate: fields
				.optional("loader_version_predicate")
				.cloned()
				.map(Predicate::from_value)
				.transpose()?,
			accepted_version_predicate: fields
				.optional("accepted_version_predicate")
				.cloned()
				.map(Predicate::from_value)
				.transpose()?,
			qualification: Qualification::parse(&qualification_text)
				.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "qualification"))?,
			declared_by: DeclaredBy::from_value(fields.required("declared_by")?.clone())?,
			evidence_digest: fields
				.optional("evidence_digest")
				.map(|v| expect_bytes(v, "evidence_digest"))
				.transpose()?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		acceptance.validate()?;
		Ok(acceptance)
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoaderObject {
	Definition(LoaderDef),
	Release(LoaderRelease),
	Acceptance(LoaderAcceptance),
}

impl Canonical for LoaderObject {
	fn to_value(&self) -> Value {
		match self {
			Self::Definition(def) => def.to_value(),
			Self::Release(release) => release.to_value(),
			Self::Acceptance(acceptance) => acceptance.to_value(),
		}
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let discriminant = value
			.get("type")
			.and_then(Value::as_text)
			.ok_or_else(|| ModelError::field(RejectReason::MissingField, "type"))?;
		match discriminant {
			"definition" => Ok(Self::Definition(LoaderDef::from_value(value)?)),
			"release" => Ok(Self::Release(LoaderRelease::from_value(value)?)),
			"mapping" => Ok(Self::Acceptance(LoaderAcceptance::from_value(value)?)),
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

fn reject_duplicate_ids<'a>(ids: impl Iterator<Item = &'a str>, key: &str) -> Result<(), ModelError> {
	let mut seen = std::collections::HashSet::new();
	for id in ids {
		if !seen.insert(id) {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, key));
		}
	}
	Ok(())
}
