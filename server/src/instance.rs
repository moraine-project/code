use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::capability::Capability;
use crate::config::Config;
use crate::routes::AppState;

const MAX_NAME: usize = 64;
const MAX_URL: usize = 2_048;
const MAX_LABEL: usize = 64;
const MAX_NAV_LINKS: usize = 16;
const MAX_THEME_TOKENS: usize = 64;

pub const COLOR_TOKENS: &[&str] = &[
	"base-100",
	"base-200",
	"base-300",
	"base-content",
	"primary",
	"primary-content",
	"secondary",
	"secondary-content",
	"accent",
	"accent-content",
	"neutral",
	"neutral-content",
	"info",
	"info-content",
	"success",
	"success-content",
	"warning",
	"warning-content",
	"error",
	"error-content",
];

pub const SHAPE_TOKENS: &[&str] = &[
	"radius-selector",
	"radius-field",
	"radius-box",
	"size-selector",
	"size-field",
	"border",
	"depth",
	"noise",
];

pub fn css_variable(token: &str) -> Option<String> {
	if COLOR_TOKENS.contains(&token) {
		return Some(format!("--color-{token}"));
	}
	if SHAPE_TOKENS.contains(&token) {
		return Some(format!("--{token}"));
	}
	None
}

fn valid_color(value: &str) -> bool {
	if value.is_empty() || value.len() > 64 {
		return false;
	}
	if let Some(hex) = value.strip_prefix('#') {
		return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|byte| byte.is_ascii_hexdigit());
	}
	let Some(open) = value.find('(') else {
		return false;
	};
	if !value.ends_with(')') {
		return false;
	}
	if !matches!(
		&value[..open],
		"rgb" | "rgba" | "hsl" | "hsla" | "hwb" | "lab" | "lch" | "oklab" | "oklch" | "color"
	) {
		return false;
	}
	let inner = &value[open + 1..value.len() - 1];
	!inner.is_empty()
		&& inner
			.bytes()
			.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'%' | b'.' | b',' | b'/' | b'+' | b'-' | b' '))
}

fn valid_length(value: &str) -> bool {
	if value.is_empty() || value.len() > 32 {
		return false;
	}
	let mut digits = false;
	let mut dot = false;
	let mut unit = "";
	for (index, character) in value.char_indices() {
		if character.is_ascii_digit() {
			digits = true;
		} else if character == '.' && digits && !dot {
			dot = true;
		} else {
			unit = &value[index..];
			break;
		}
	}
	digits && matches!(unit, "" | "px" | "rem" | "em" | "%")
}

pub fn valid_token(token: &str, value: &str) -> bool {
	if COLOR_TOKENS.contains(&token) {
		valid_color(value)
	} else if SHAPE_TOKENS.contains(&token) {
		valid_length(value)
	} else {
		false
	}
}

pub fn valid_href(href: &str) -> bool {
	if href.is_empty() || href.len() > MAX_URL {
		return false;
	}
	if href.starts_with('/') && !href.starts_with("//") {
		return true;
	}
	href.starts_with("https://") || href.starts_with("http://")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavLink {
	pub label: String,
	pub href: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Branding {
	#[serde(default)]
	pub name: Option<String>,
	#[serde(default)]
	pub logo: Option<String>,
	#[serde(default)]
	pub theme: BTreeMap<String, String>,
	#[serde(default)]
	pub nav: Vec<NavLink>,
}

impl Branding {
	pub fn validate(&self) -> Result<(), String> {
		if let Some(name) = &self.name
			&& (name.trim().is_empty() || name.chars().count() > MAX_NAME)
		{
			return Err(format!("name must be 1 to {MAX_NAME} characters"));
		}
		if let Some(logo) = &self.logo
			&& (logo.trim().is_empty() || logo.len() > MAX_URL)
		{
			return Err(format!("logo must be a URL of at most {MAX_URL} bytes"));
		}
		if self.theme.len() > MAX_THEME_TOKENS {
			return Err(format!("theme accepts at most {MAX_THEME_TOKENS} tokens"));
		}
		for (token, value) in &self.theme {
			if css_variable(token).is_none() {
				return Err(format!("{token} is not a known theme token"));
			}
			if !valid_token(token, value) {
				return Err(format!("{token} has an unusable value"));
			}
		}
		if self.nav.len() > MAX_NAV_LINKS {
			return Err(format!("nav accepts at most {MAX_NAV_LINKS} links"));
		}
		for link in &self.nav {
			if link.label.trim().is_empty() || link.label.chars().count() > MAX_LABEL {
				return Err(format!("a nav label must be 1 to {MAX_LABEL} characters"));
			}
			if !valid_href(&link.href) {
				return Err("a nav href must be a site-relative path or an http(s) URL".to_string());
			}
		}
		Ok(())
	}
}

#[derive(Debug, Clone, Serialize)]
pub struct Instance {
	pub capabilities: Capability,
	pub branding: Branding,
}

#[derive(Clone, Default)]
pub struct BrandingSource {
	current: Arc<RwLock<Branding>>,
}

impl BrandingSource {
	pub fn current(&self) -> Branding {
		self.current.read().unwrap_or_else(|error| error.into_inner()).clone()
	}

	pub fn replace(&self, branding: Branding) -> Result<(), String> {
		branding.validate()?;
		*self.current.write().unwrap_or_else(|error| error.into_inner()) = branding;
		Ok(())
	}
}

pub fn routes() -> Router<AppState> {
	Router::new().route("/v1/instance", get(instance))
}

async fn instance(State(state): State<AppState>) -> Json<Instance> {
	Json(Instance {
		capabilities: (*state.capability).clone(),
		branding: state.branding.current(),
	})
}

pub fn load(config: &Config) -> BrandingSource {
	let source = BrandingSource::default();
	let Some(path) = config.instance_branding.as_deref() else {
		return source;
	};
	let branding = match std::fs::read_to_string(path) {
		Ok(text) => match serde_json::from_str::<Branding>(&text) {
			Ok(branding) => branding,
			Err(error) => {
				tracing::error!(path = %path.display(), %error, "the branding file is malformed; the default branding is used");
				return source;
			}
		},
		Err(error) => {
			tracing::error!(path = %path.display(), %error, "the branding file could not be read; the default branding is used");
			return source;
		}
	};
	if let Err(error) = source.replace(branding) {
		tracing::error!(path = %path.display(), %error, "the branding file is out of contract; the default branding is used");
	}
	source
}

#[cfg(test)]
mod tests {
	use clap::Parser;

	use super::*;

	fn branding_from(text: &str) -> Result<Branding, String> {
		let branding = serde_json::from_str::<Branding>(text).expect("the fixture parses");
		branding.validate()?;
		Ok(branding)
	}

	fn source_with(text: &str) -> BrandingSource {
		let source = BrandingSource::default();
		source
			.replace(branding_from(text).expect("the fixture is in contract"))
			.expect("replace");
		source
	}

	#[test]
	fn defaults_when_the_file_is_absent() {
		let config = Config::parse_from(["moraine-server"]);
		assert_eq!(load(&config).current(), Branding::default());
	}

	#[test]
	fn reads_a_complete_branding_document() {
		let source = source_with(
			r##"{
				"name": "Example Registry",
				"logo": "https://cdn.example/logo.svg",
				"theme": { "primary": "#ff0000", "radius-box": "1rem" },
				"nav": [{ "label": "Docs", "href": "https://docs.example" }]
			}"##,
		);
		let branding = source.current();
		assert_eq!(branding.name.as_deref(), Some("Example Registry"));
		assert_eq!(branding.logo.as_deref(), Some("https://cdn.example/logo.svg"));
		assert_eq!(branding.theme.get("primary").map(String::as_str), Some("#ff0000"));
		assert_eq!(branding.nav.len(), 1);
	}

	#[test]
	fn rejects_unknown_keys() {
		let error = serde_json::from_str::<Branding>(r#"{ "colour": "red" }"#).unwrap_err();
		assert!(error.to_string().contains("colour"));
	}

	#[test]
	fn accepts_every_documented_token_name() {
		for token in COLOR_TOKENS.iter().chain(SHAPE_TOKENS) {
			let value = if COLOR_TOKENS.contains(token) { "#123456" } else { "1rem" };
			assert!(css_variable(token).is_some(), "{token} has no property");
			assert!(valid_token(token, value), "{token} rejected {value}");
		}
	}

	#[test]
	fn refuses_a_token_outside_the_contract() {
		let error = branding_from(r#"{ "theme": { "background-image": "url(x)" } }"#).unwrap_err();
		assert!(error.contains("background-image"), "{error}");
	}

	#[test]
	fn refuses_a_colour_that_could_carry_a_declaration() {
		for value in ["red; color: blue", "#fff}", "url(https://x)", "calc(1 + 1)", " #fff"] {
			let text = format!(r#"{{ "theme": {{ "primary": {value:?} }} }}"#);
			assert!(branding_from(&text).is_err(), "primary accepted {value:?}");
		}
	}

	#[test]
	fn accepts_the_colour_notations_the_client_also_accepts() {
		for value in [
			"#fff",
			"#ffff",
			"#ff0000",
			"#ff0000aa",
			"oklch(70% 0.15 250)",
			"rgb(1 2 3 / 50%)",
		] {
			let text = format!(r#"{{ "theme": {{ "primary": {value:?} }} }}"#);
			assert!(branding_from(&text).is_ok(), "primary rejected {value:?}");
		}
	}

	#[test]
	fn refuses_a_shape_that_is_not_a_length() {
		let error = branding_from(r#"{ "theme": { "border": "medium" } }"#).unwrap_err();
		assert!(error.contains("border"), "{error}");
	}

	#[test]
	fn refuses_a_nav_href_that_leaves_the_contract() {
		for href in ["javascript:alert(1)", "//evil.example", "data:text/html,x"] {
			let text = format!(r#"{{ "nav": [{{ "label": "X", "href": {href:?} }}] }}"#);
			assert!(branding_from(&text).is_err(), "nav accepted {href:?}");
		}
	}

	#[test]
	fn accepts_a_site_relative_nav_href() {
		let text = r#"{ "nav": [{ "label": "Rules", "href": "/about" }] }"#;
		assert_eq!(branding_from(text).expect("in contract").nav[0].href, "/about");
	}

	#[test]
	fn refuses_a_blank_or_oversized_name() {
		assert!(branding_from(r#"{ "name": "   " }"#).is_err());
		let long = "x".repeat(MAX_NAME + 1);
		assert!(branding_from(&format!(r#"{{ "name": {long:?} }}"#)).is_err());
	}

	#[test]
	fn caps_the_number_of_nav_links() {
		let links: Vec<String> = (0..MAX_NAV_LINKS + 5)
			.map(|index| format!(r#"{{ "label": "L{index}", "href": "/{index}" }}"#))
			.collect();
		let text = format!(r#"{{ "nav": [{}] }}"#, links.join(","));
		assert!(branding_from(&text).is_err());
	}

	#[test]
	fn a_rejected_replacement_leaves_the_previous_branding_in_place() {
		let source = source_with(r#"{ "name": "Kept" }"#);
		let hostile = serde_json::from_str::<Branding>(r#"{ "theme": { "primary": "x" } }"#).expect("the fixture parses");
		assert!(source.replace(hostile).is_err());
		assert_eq!(source.current().name.as_deref(), Some("Kept"));
	}

	#[test]
	fn a_replacement_becomes_the_current_branding() {
		let source = BrandingSource::default();
		source
			.replace(branding_from(r#"{ "name": "Second" }"#).expect("in contract"))
			.expect("replace");
		assert_eq!(source.current().name.as_deref(), Some("Second"));
	}
}
