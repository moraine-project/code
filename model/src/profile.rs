use moraine_codec::Value;

use crate::canonical::{
	Canonical, Fields, expect_array, expect_bytes, expect_i64, expect_text, expect_text_array, expect_u32, map_of,
};
use crate::compatibility::Rights;
use crate::error::{ModelError, RejectReason};
use crate::reference::ArtifactRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
	pub kind: String,
	pub url: String,
}

impl Canonical for Link {
	fn to_value(&self) -> Value {
		map_of(
			"Link",
			[
				("kind", Value::text(self.kind.clone())),
				("url", Value::text(self.url.clone())),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Link", value)?.reject_unknown(&["kind", "url"])?;
		Ok(Self {
			kind: expect_text(fields.required("kind")?, "kind")?,
			url: expect_text(fields.required("url")?, "url")?,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRevision {
	pub protocol: u32,
	pub project_id: String,
	pub game_id: String,
	pub revision_nonce: Vec<u8>,
	pub display_name: String,
	pub summary: String,
	pub description: String,
	pub icon: Option<ArtifactRef>,
	pub gallery: Vec<ArtifactRef>,
	pub links: Vec<Link>,
	pub communities: Vec<Link>,
	pub categories: Vec<String>,
	pub tags: Vec<String>,
	pub rights: Option<Rights>,
	pub declared_time: i64,
}

impl ProfileRevision {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.revision_nonce.len() < 16 {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"revision_nonce must be at least 16 bytes",
			));
		}
		if self.project_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "project_id"));
		}
		if self.game_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "game_id"));
		}
		if self.display_name.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "display_name"));
		}
		if let Some(icon) = &self.icon {
			icon.validate()?;
		}
		for artifact in &self.gallery {
			artifact.validate()?;
		}
		Ok(())
	}
}

impl Canonical for ProfileRevision {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("project_id", Value::text(self.project_id.clone())),
			("game_id", Value::text(self.game_id.clone())),
			("revision_nonce", Value::bytes(self.revision_nonce.clone())),
			("display_name", Value::text(self.display_name.clone())),
			("summary", Value::text(self.summary.clone())),
			("description", Value::text(self.description.clone())),
		];
		if let Some(icon) = &self.icon {
			pairs.push(("icon", icon.to_value()));
		}
		pairs.push((
			"gallery",
			Value::array(self.gallery.iter().map(Canonical::to_value).collect::<Vec<_>>()),
		));
		pairs.push((
			"links",
			Value::array(self.links.iter().map(Canonical::to_value).collect::<Vec<_>>()),
		));
		pairs.push((
			"communities",
			Value::array(self.communities.iter().map(Canonical::to_value).collect::<Vec<_>>()),
		));
		pairs.push((
			"categories",
			Value::array(self.categories.iter().cloned().map(Value::text).collect::<Vec<_>>()),
		));
		pairs.push((
			"tags",
			Value::array(self.tags.iter().cloned().map(Value::text).collect::<Vec<_>>()),
		));
		if let Some(rights) = &self.rights {
			pairs.push(("rights", rights.to_value()));
		}
		pairs.push(("declared_time", Value::int(self.declared_time)));
		map_of("ProfileRevision", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("ProfileRevision", value)?.reject_unknown(&[
			"protocol",
			"project_id",
			"game_id",
			"revision_nonce",
			"display_name",
			"summary",
			"description",
			"icon",
			"gallery",
			"links",
			"communities",
			"categories",
			"tags",
			"rights",
			"declared_time",
		])?;
		let profile = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			game_id: expect_text(fields.required("game_id")?, "game_id")?,
			revision_nonce: expect_bytes(fields.required("revision_nonce")?, "revision_nonce")?,
			display_name: expect_text(fields.required("display_name")?, "display_name")?,
			summary: expect_text(fields.required("summary")?, "summary")?,
			description: expect_text(fields.required("description")?, "description")?,
			icon: fields.optional("icon").cloned().map(ArtifactRef::from_value).transpose()?,
			gallery: expect_array(fields.required("gallery")?, "gallery")?
				.iter()
				.cloned()
				.map(ArtifactRef::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			links: expect_array(fields.required("links")?, "links")?
				.iter()
				.cloned()
				.map(Link::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			communities: expect_array(fields.required("communities")?, "communities")?
				.iter()
				.cloned()
				.map(Link::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			categories: expect_text_array(fields.required("categories")?, "categories")?,
			tags: expect_text_array(fields.required("tags")?, "tags")?,
			rights: fields.optional("rights").cloned().map(Rights::from_value).transpose()?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		profile.validate()?;
		Ok(profile)
	}
}
