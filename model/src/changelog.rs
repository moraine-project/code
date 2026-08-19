use moraine_codec::Value;

use crate::canonical::{Canonical, Fields, expect_array, expect_i64, expect_text, expect_u32, map_of};
use crate::error::{ModelError, RejectReason};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogSection {
	pub heading: String,
	pub body: String,
	pub severity: Option<String>,
}

impl Canonical for ChangelogSection {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("heading", Value::text(self.heading.clone())),
			("body", Value::text(self.body.clone())),
		];
		if let Some(severity) = &self.severity {
			pairs.push(("severity", Value::text(severity.clone())));
		}
		map_of("ChangelogSection", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("ChangelogSection", value)?.reject_unknown(&["heading", "body", "severity"])?;
		Ok(Self {
			heading: expect_text(fields.required("heading")?, "heading")?,
			body: expect_text(fields.required("body")?, "body")?,
			severity: fields.optional("severity").map(|v| expect_text(v, "severity")).transpose()?,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleSection {
	pub locale: String,
	pub sections: Vec<ChangelogSection>,
}

impl Canonical for LocaleSection {
	fn to_value(&self) -> Value {
		map_of(
			"LocaleSection",
			[
				("locale", Value::text(self.locale.clone())),
				(
					"sections",
					Value::array(self.sections.iter().map(Canonical::to_value).collect::<Vec<_>>()),
				),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("LocaleSection", value)?.reject_unknown(&["locale", "sections"])?;
		Ok(Self {
			locale: expect_text(fields.required("locale")?, "locale")?,
			sections: expect_array(fields.required("sections")?, "sections")?
				.iter()
				.cloned()
				.map(ChangelogSection::from_value)
				.collect::<Result<Vec<_>, _>>()?,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changelog {
	pub protocol: u32,
	pub project_id: String,
	pub release_id: Option<String>,
	pub locale_sections: Vec<LocaleSection>,
	pub declared_time: i64,
}

impl Changelog {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "protocol"));
		}
		if self.project_id.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "project_id"));
		}
		if self.locale_sections.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "locale_sections"));
		}
		for locale in &self.locale_sections {
			if locale.locale.is_empty() {
				return Err(ModelError::field(RejectReason::InvalidFieldValue, "locale"));
			}
		}
		Ok(())
	}

	pub fn text(&self) -> String {
		let mut text = String::new();
		for locale in &self.locale_sections {
			for section in &locale.sections {
				if !text.is_empty() {
					text.push('\n');
				}
				text.push_str(&section.heading);
				text.push('\n');
				text.push_str(&section.body);
			}
		}
		text
	}
}

impl Canonical for Changelog {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("protocol", Value::int(i64::from(self.protocol))),
			("project_id", Value::text(self.project_id.clone())),
		];
		if let Some(release_id) = &self.release_id {
			pairs.push(("release_id", Value::text(release_id.clone())));
		}
		pairs.push((
			"locale_sections",
			Value::array(self.locale_sections.iter().map(Canonical::to_value).collect::<Vec<_>>()),
		));
		pairs.push(("declared_time", Value::int(self.declared_time)));
		map_of("Changelog", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Changelog", value)?.reject_unknown(&[
			"protocol",
			"project_id",
			"release_id",
			"locale_sections",
			"declared_time",
		])?;
		let changelog = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			release_id: fields
				.optional("release_id")
				.map(|v| expect_text(v, "release_id"))
				.transpose()?,
			locale_sections: expect_array(fields.required("locale_sections")?, "locale_sections")?
				.iter()
				.cloned()
				.map(LocaleSection::from_value)
				.collect::<Result<Vec<_>, _>>()?,
			declared_time: expect_i64(fields.required("declared_time")?, "declared_time")?,
		};
		changelog.validate()?;
		Ok(changelog)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn changelog() -> Changelog {
		Changelog {
			protocol: 1,
			project_id: "p".to_string(),
			release_id: Some("r".to_string()),
			locale_sections: vec![LocaleSection {
				locale: "en".to_string(),
				sections: vec![ChangelogSection {
					heading: "Fixes".to_string(),
					body: "Corrected a crash on launch".to_string(),
					severity: Some("high".to_string()),
				}],
			}],
			declared_time: 1_760_000_000,
		}
	}

	#[test]
	fn round_trips_through_canonical_bytes() {
		let changelog = changelog();
		let bytes = changelog.to_canonical_bytes();
		assert_eq!(Changelog::from_canonical_bytes(&bytes).expect("decode"), changelog);
	}

	#[test]
	fn flattens_the_sections_into_searchable_text() {
		let text = changelog().text();
		assert!(text.contains("Fixes"));
		assert!(text.contains("Corrected a crash on launch"));
	}

	#[test]
	fn rejects_an_empty_locale_list() {
		let mut changelog = changelog();
		changelog.locale_sections.clear();
		assert!(changelog.validate().is_err());
	}
}
