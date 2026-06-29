use moraine_codec::Value;

use crate::canonical::{Canonical, Fields, expect_bool, expect_text, expect_text_array, map_of};
use crate::error::{ModelError, RejectReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
	Exact,
	Set,
	Semver,
	OrderedList,
	Calendar,
	Any,
}

impl Scheme {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Exact => "exact",
			Self::Set => "set",
			Self::Semver => "semver",
			Self::OrderedList => "ordered-list",
			Self::Calendar => "calendar",
			Self::Any => "any",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"exact" => Self::Exact,
			"set" => Self::Set,
			"semver" => Self::Semver,
			"ordered-list" => Self::OrderedList,
			"calendar" => Self::Calendar,
			"any" => Self::Any,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredicateResult {
	Satisfied,
	NotSatisfied,
	Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Predicate {
	pub scheme: String,
	pub values: Vec<String>,
}

impl Predicate {
	pub fn new(scheme: Scheme, values: Vec<String>) -> Self {
		Self {
			scheme: scheme.as_str().to_string(),
			values,
		}
	}

	pub fn scheme(&self) -> Option<Scheme> {
		Scheme::parse(&self.scheme)
	}
}

impl Canonical for Predicate {
	fn to_value(&self) -> Value {
		map_of(
			"Predicate",
			[
				("scheme", Value::text(self.scheme.clone())),
				(
					"values",
					Value::array(self.values.iter().cloned().map(Value::text).collect::<Vec<_>>()),
				),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Predicate", value)?.reject_unknown(&["scheme", "values"])?;
		Ok(Self {
			scheme: expect_text(fields.required("scheme")?, "scheme")?,
			values: expect_text_array(fields.required("values")?, "values")?,
		})
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
	Client,
	Server,
	Both,
}

impl Side {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Client => "client",
			Self::Server => "server",
			Self::Both => "both",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"client" => Self::Client,
			"server" => Self::Server,
			"both" => Self::Both,
			_ => return None,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compatibility {
	pub game_version_predicate: Predicate,
	pub loader_id: Option<String>,
	pub loader_version_predicate: Option<Predicate>,
	pub side: Side,
	pub runtime_predicate: Option<Predicate>,
	pub os_predicate: Option<Vec<String>>,
	pub arch_predicate: Option<Vec<String>>,
}

impl Canonical for Compatibility {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("game_version_predicate", self.game_version_predicate.to_value()),
			("side", Value::text(self.side.as_str())),
		];
		if let Some(loader_id) = &self.loader_id {
			pairs.push(("loader_id", Value::text(loader_id.clone())));
		}
		if let Some(predicate) = &self.loader_version_predicate {
			pairs.push(("loader_version_predicate", predicate.to_value()));
		}
		if let Some(predicate) = &self.runtime_predicate {
			pairs.push(("runtime_predicate", predicate.to_value()));
		}
		if let Some(os) = &self.os_predicate {
			pairs.push((
				"os_predicate",
				Value::array(os.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			));
		}
		if let Some(arch) = &self.arch_predicate {
			pairs.push((
				"arch_predicate",
				Value::array(arch.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			));
		}
		map_of("Compatibility", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Compatibility", value)?.reject_unknown(&[
			"game_version_predicate",
			"loader_id",
			"loader_version_predicate",
			"side",
			"runtime_predicate",
			"os_predicate",
			"arch_predicate",
		])?;
		let side_text = expect_text(fields.required("side")?, "side")?;
		let side = Side::parse(&side_text).ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, "side"))?;
		if let Some(loader_id) = fields.optional("loader_id") {
			expect_text(loader_id, "loader_id")?;
		}
		Ok(Self {
			game_version_predicate: Predicate::from_value(fields.required("game_version_predicate")?.clone())?,
			loader_id: fields
				.optional("loader_id")
				.map(|v| expect_text(v, "loader_id"))
				.transpose()?,
			loader_version_predicate: fields
				.optional("loader_version_predicate")
				.cloned()
				.map(Predicate::from_value)
				.transpose()?,
			side,
			runtime_predicate: fields
				.optional("runtime_predicate")
				.cloned()
				.map(Predicate::from_value)
				.transpose()?,
			os_predicate: fields
				.optional("os_predicate")
				.map(|v| expect_text_array(v, "os_predicate"))
				.transpose()?,
			arch_predicate: fields
				.optional("arch_predicate")
				.map(|v| expect_text_array(v, "arch_predicate"))
				.transpose()?,
		})
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
	Yes,
	No,
	Ask,
}

impl Permission {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Yes => "yes",
			Self::No => "no",
			Self::Ask => "ask",
		}
	}

	pub fn parse(value: &str) -> Option<Self> {
		Some(match value {
			"yes" => Self::Yes,
			"no" => Self::No,
			"ask" => Self::Ask,
			_ => return None,
		})
	}
}

fn expect_permission(value: &Value, key: &str) -> Result<Permission, ModelError> {
	let text = expect_text(value, key)?;
	Permission::parse(&text).ok_or_else(|| ModelError::field(RejectReason::InvalidFieldValue, key))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rights {
	pub redistribution: Permission,
	pub modpack_inclusion: Permission,
	pub commercial_use: Permission,
	pub server_use: Permission,
	pub mirroring: Permission,
	pub attribution_required: bool,
	pub notes: Option<String>,
}

impl Canonical for Rights {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("redistribution", Value::text(self.redistribution.as_str())),
			("modpack_inclusion", Value::text(self.modpack_inclusion.as_str())),
			("commercial_use", Value::text(self.commercial_use.as_str())),
			("server_use", Value::text(self.server_use.as_str())),
			("mirroring", Value::text(self.mirroring.as_str())),
			("attribution_required", Value::Bool(self.attribution_required)),
		];
		if let Some(notes) = &self.notes {
			pairs.push(("notes", Value::text(notes.clone())));
		}
		map_of("Rights", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Rights", value)?.reject_unknown(&[
			"redistribution",
			"modpack_inclusion",
			"commercial_use",
			"server_use",
			"mirroring",
			"attribution_required",
			"notes",
		])?;
		Ok(Self {
			redistribution: expect_permission(fields.required("redistribution")?, "redistribution")?,
			modpack_inclusion: expect_permission(fields.required("modpack_inclusion")?, "modpack_inclusion")?,
			commercial_use: expect_permission(fields.required("commercial_use")?, "commercial_use")?,
			server_use: expect_permission(fields.required("server_use")?, "server_use")?,
			mirroring: expect_permission(fields.required("mirroring")?, "mirroring")?,
			attribution_required: expect_bool(fields.required("attribution_required")?, "attribution_required")?,
			notes: fields.optional("notes").map(|v| expect_text(v, "notes")).transpose()?,
		})
	}
}
