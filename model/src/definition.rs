use moraine_codec::Value;

use crate::canonical::{
	Canonical, Fields, expect_array, expect_bool, expect_i64, expect_text, expect_text_array, expect_u32, map_of,
};
use crate::error::{ModelError, RejectReason};
use crate::version::{OrderingScheme, VersionCatalog};

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
	pub version_catalog: Vec<String>,
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

	pub fn catalog(&self) -> Option<VersionCatalog> {
		Some(VersionCatalog::new(self.ordering()?, self.version_catalog.clone()))
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
		if self.version_catalog.iter().any(String::is_empty) {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "version_catalog"));
		}
		reject_duplicate_ids(self.version_catalog.iter().map(String::as_str), "version_catalog")?;
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
		if !self.version_catalog.is_empty() {
			pairs.push((
				"version_catalog",
				Value::array(self.version_catalog.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			));
		}
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
			"version_catalog",
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
			version_catalog: fields
				.optional("version_catalog")
				.map(|value| expect_text_array(value, "version_catalog"))
				.transpose()?
				.unwrap_or_default(),
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
	pub version_catalog: Vec<String>,
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
		if self.version_catalog.iter().any(String::is_empty) {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "version_catalog"));
		}
		reject_duplicate_ids(self.version_catalog.iter().map(String::as_str), "version_catalog")?;
		Ok(())
	}

	pub fn catalog(&self) -> Option<VersionCatalog> {
		Some(VersionCatalog::new(self.ordering()?, self.version_catalog.clone()))
	}

	pub fn ordering(&self) -> Option<OrderingScheme> {
		OrderingScheme::parse(&self.version_ordering)
	}
}

impl Canonical for RuntimeDef {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("runtime_id", Value::text(self.runtime_id.clone())),
			("kind", Value::text(self.kind.clone())),
			("display_name", Value::text(self.display_name.clone())),
			("version_ordering", Value::text(self.version_ordering.clone())),
		];
		if !self.version_catalog.is_empty() {
			pairs.push((
				"version_catalog",
				Value::array(self.version_catalog.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			));
		}
		pairs.push(("declared_time", Value::int(self.declared_time)));
		map_of("RuntimeDef", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("RuntimeDef", value)?.reject_unknown(&[
			"protocol",
			"runtime_id",
			"kind",
			"display_name",
			"version_ordering",
			"version_catalog",
			"declared_time",
		])?;
		let def = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			runtime_id: expect_text(fields.required("runtime_id")?, "runtime_id")?,
			kind: expect_text(fields.required("kind")?, "kind")?,
			display_name: expect_text(fields.required("display_name")?, "display_name")?,
			version_ordering: expect_text(fields.required("version_ordering")?, "version_ordering")?,
			version_catalog: fields
				.optional("version_catalog")
				.map(|value| expect_text_array(value, "version_catalog"))
				.transpose()?
				.unwrap_or_default(),
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		def.validate()?;
		Ok(def)
	}
}

mod loader;

pub use loader::{DeclaredBy, LoaderAcceptance, LoaderDef, LoaderObject, LoaderRelease, Qualification};

fn reject_duplicate_ids<'a>(ids: impl Iterator<Item = &'a str>, key: &str) -> Result<(), ModelError> {
	let mut seen = std::collections::HashSet::new();
	for id in ids {
		if !seen.insert(id) {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, key));
		}
	}
	Ok(())
}
