pub(crate) mod lock;
mod push;
mod signing;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use lock::write_lock;
use moraine_crypto::ObjectKind;
use moraine_model::compatibility::Predicate;
use moraine_model::definition::{
	Category as GameCategory, DeclaredBy, GameDef, LoaderAcceptance, LoaderDef, LoaderObject, LoaderRelease, Qualification,
	RuntimeDef, Tag as GameTag, VersionSyntax,
};
use moraine_model::genesis::GenesisKind;
use moraine_model::signed::sign_payload;
pub use push::push;
use serde::Deserialize;
use signing::{emit, now, optional_revision, revision, revision_id, write_object};
pub use signing::{game, loader, loader_acceptance, loader_release, runtime};

use crate::keyfile;

fn trimmed(value: Option<&str>) -> Option<String> {
	value.map(str::trim).filter(|text| !text.is_empty()).map(str::to_string)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DefinitionFile {
	kind: String,
	#[serde(default)]
	name: Option<String>,
	#[serde(default)]
	display_name: Option<String>,
	#[serde(default)]
	version_ordering: Option<String>,
	#[serde(default)]
	loaders_allowed: Option<bool>,
	#[serde(default)]
	loader_authorities: Vec<String>,
	#[serde(default)]
	categories: Vec<CategoryFile>,
	#[serde(default)]
	tags: Vec<TagFile>,
	#[serde(default)]
	game_id: Option<String>,
	#[serde(default)]
	revision_of: Option<String>,
	#[serde(default)]
	runtime_kind: Option<String>,
	#[serde(default)]
	metadata_extractor: Option<String>,
	#[serde(default)]
	install_adapter: Option<String>,
	#[serde(default)]
	versions: Vec<String>,
	#[serde(default)]
	game_versions: Vec<String>,
	#[serde(default)]
	game_version_scheme: Option<String>,
	#[serde(default)]
	runtime_versions: Vec<String>,
	#[serde(default)]
	loader_id: Option<String>,
	#[serde(default)]
	version: Option<String>,
	#[serde(default)]
	runtime_id: Option<String>,
	#[serde(default)]
	accepting_loader: Option<String>,
	#[serde(default)]
	accepted_loader: Option<String>,
	#[serde(default)]
	accepted_versions: Vec<String>,
	#[serde(default)]
	qualification: Option<String>,
	#[serde(default)]
	declared_by: Option<String>,
	#[serde(default)]
	declared_by_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CategoryFile {
	id: String,
	label: String,
	#[serde(default)]
	parent: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TagFile {
	id: String,
	label: String,
}

pub fn from_file(key_path: &Path, file: &Path, out: &Path) -> Result<(), String> {
	let text = std::fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
	let source: DefinitionFile = toml::from_str(&text).map_err(|error| format!("{}: {error}", file.display()))?;
	let stem = file
		.file_stem()
		.and_then(|stem| stem.to_str())
		.unwrap_or_default()
		.to_string();
	let mut names = BTreeMap::new();
	compile(key_path, &source, &stem, out, &mut names).map(|_| ())
}

pub fn from_directory(key_path: &Path, directory: &Path, out: &Path) -> Result<(), String> {
	let mut entries = Vec::new();
	collect_definition_files(directory, &mut entries)?;
	entries.sort_by_key(|(_, source)| compile_order(&source.kind));
	let mut names = BTreeMap::new();
	for (path, source) in &entries {
		let stem = path
			.file_stem()
			.and_then(|stem| stem.to_str())
			.unwrap_or_default()
			.to_string();
		compile(key_path, source, &stem, out, &mut names)?;
	}
	Ok(())
}

fn compile_order(kind: &str) -> u8 {
	match kind {
		"game" => 0,
		"runtime" => 1,
		"loader" => 2,
		"loader-release" | "release" => 3,
		"mapping" => 4,
		_ => 5,
	}
}

fn collect_definition_files(directory: &Path, out: &mut Vec<(PathBuf, DefinitionFile)>) -> Result<(), String> {
	let entries = std::fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
	let mut subdirectories = Vec::new();
	for entry in entries {
		let entry = entry.map_err(|error| error.to_string())?;
		let path = entry.path();
		if path.is_dir() {
			subdirectories.push(path);
			continue;
		}
		if !path.extension().is_some_and(|extension| extension == "toml") {
			continue;
		}
		let text = std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
		let source: DefinitionFile = toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
		out.push((path, source));
	}
	for subdirectory in subdirectories {
		collect_definition_files(&subdirectory, out)?;
	}
	out.sort_by(|left, right| left.0.cmp(&right.0));
	Ok(())
}

fn resolve_ref(names: &BTreeMap<String, String>, value: &str) -> Result<String, String> {
	if value.starts_with("gd:sha256:") {
		return Ok(value.to_string());
	}
	names
		.get(value)
		.cloned()
		.ok_or_else(|| format!("`{value}` is not a definition name defined earlier in this directory"))
}

fn compile(
	key_path: &Path,
	source: &DefinitionFile,
	default_name: &str,
	out: &Path,
	names: &mut BTreeMap<String, String>,
) -> Result<String, String> {
	let key = keyfile::load(key_path)?;
	let display_name = source.display_name.as_deref().unwrap_or_default().trim().to_string();
	let ordering = source.version_ordering.as_deref().unwrap_or_default().trim().to_string();
	if matches!(source.kind.as_str(), "game" | "loader" | "runtime") && (display_name.is_empty() || ordering.is_empty()) {
		return Err("display_name and version_ordering are required".to_string());
	}
	if !matches!(source.kind.as_str(), "game" | "loader" | "runtime")
		&& optional_revision(source.revision_of.as_deref()).is_some()
	{
		return Err("revision_of applies to a game, loader, or runtime definition".to_string());
	}
	let definition_id = match source.kind.as_str() {
		"game" => {
			let authorities = source
				.loader_authorities
				.iter()
				.map(|authority| resolve_ref(names, authority))
				.collect::<Result<Vec<_>, _>>()?;
			let build = |game_id: &str| GameDef {
				protocol: 1,
				game_id: game_id.to_string(),
				display_name: display_name.clone(),
				version_syntax: VersionSyntax {
					kind: ordering.clone(),
					pattern: None,
				},
				version_ordering: ordering.clone(),
				version_catalog: source.versions.clone(),
				loaders_allowed: source.loaders_allowed.unwrap_or(true),
				loader_authorities: authorities,
				categories: source
					.categories
					.iter()
					.map(|category| GameCategory {
						id: category.id.clone(),
						label: category.label.clone(),
						parent: category.parent.clone(),
					})
					.collect(),
				tags: source
					.tags
					.iter()
					.map(|tag| GameTag {
						id: tag.id.clone(),
						label: tag.label.clone(),
					})
					.collect(),
				metadata_extractor: trimmed(source.metadata_extractor.as_deref()),
				install_adapter: trimmed(source.install_adapter.as_deref()),
				declared_time: now(),
			};
			match optional_revision(source.revision_of.as_deref()) {
				Some(id) => revision(&key, ObjectKind::GameDef, &revision_id(&id)?, build, out, "game_id")?,
				None => emit(
					&key,
					GenesisKind::Game,
					&["delegation", "game-def"],
					ObjectKind::GameDef,
					build,
					out,
					"game_id",
				)?,
			}
		}
		"loader" => {
			let game_id = resolve_ref(names, required_field(source.game_id.as_deref(), "game_id")?)?;
			let build = |loader_id: &str| {
				LoaderObject::Definition(LoaderDef {
					protocol: 1,
					loader_id: loader_id.to_string(),
					game_id: game_id.clone(),
					display_name: display_name.clone(),
					version_ordering: ordering.clone(),
					version_catalog: source.versions.clone(),
					game_versions: predicate_or_none(&source.game_versions, source.game_version_scheme.as_deref()),
					bootstrap: None,
					accepted_artifacts: None,
					declared_time: now(),
				})
			};
			match optional_revision(source.revision_of.as_deref()) {
				Some(id) => revision(&key, ObjectKind::LoaderDef, &revision_id(&id)?, build, out, "loader_id")?,
				None => emit(
					&key,
					GenesisKind::Loader,
					&["delegation", "loader-def"],
					ObjectKind::LoaderDef,
					build,
					out,
					"loader_id",
				)?,
			}
		}
		"runtime" => {
			let runtime_kind = source.runtime_kind.as_deref().unwrap_or("java");
			let build = |runtime_id: &str| RuntimeDef {
				protocol: 1,
				runtime_id: runtime_id.to_string(),
				kind: runtime_kind.to_string(),
				display_name: display_name.clone(),
				version_ordering: ordering.clone(),
				version_catalog: source.versions.clone(),
				declared_time: now(),
			};
			match optional_revision(source.revision_of.as_deref()) {
				Some(id) => revision(&key, ObjectKind::RuntimeDef, &revision_id(&id)?, build, out, "runtime_id")?,
				None => emit(
					&key,
					GenesisKind::Runtime,
					&["delegation", "runtime-def"],
					ObjectKind::RuntimeDef,
					build,
					out,
					"runtime_id",
				)?,
			}
		}
		"loader-release" | "release" => {
			let loader_id = resolve_ref(names, required_field(source.loader_id.as_deref(), "loader_id")?)?;
			let version = required_field(source.version.as_deref(), "version")?;
			if source.game_versions.is_empty() {
				return Err("a loader release needs at least one game_version".to_string());
			}
			let runtime_id = match source.runtime_id.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
				Some(value) => Some(resolve_ref(names, value)?),
				None => None,
			};
			let release = LoaderObject::Release(LoaderRelease {
				protocol: 1,
				loader_id,
				version_id: version.to_string(),
				game_version_predicate: Predicate {
					scheme: source
						.game_version_scheme
						.as_deref()
						.map(str::trim)
						.filter(|text| !text.is_empty())
						.unwrap_or("exact")
						.to_string(),
					values: source.game_versions.clone(),
				},
				runtime_id,
				runtime_predicate: predicate_or_none(&source.runtime_versions, None),
				bootstrap: None,
				declared_time: now(),
			});
			let signed = sign_payload(ObjectKind::LoaderDef, &release, &[&key]);
			let id = signed.id(ObjectKind::LoaderDef);
			write_object(out, &id, &signed.wire_bytes())?;
			println!("loader_release: {id}");
			String::new()
		}
		"mapping" => {
			let accepting_loader =
				resolve_ref(names, required_field(source.accepting_loader.as_deref(), "accepting_loader")?)?;
			let accepted_loader = resolve_ref(names, required_field(source.accepted_loader.as_deref(), "accepted_loader")?)?;
			let game_id = resolve_ref(names, required_field(source.game_id.as_deref(), "game_id")?)?;
			let qualification_text = source.qualification.as_deref().unwrap_or("most");
			let qualification = Qualification::parse(qualification_text)
				.ok_or_else(|| format!("unknown qualification `{qualification_text}`"))?;
			let declared_by_kind = source.declared_by.as_deref().unwrap_or("loader-authority");
			if !matches!(declared_by_kind, "loader-authority" | "project" | "tester") {
				return Err(format!("unknown declared_by `{declared_by_kind}`"));
			}
			let acceptance = LoaderObject::Acceptance(LoaderAcceptance {
				protocol: 1,
				accepting_loader_id: accepting_loader.clone(),
				accepted_loader_id: accepted_loader,
				game_id,
				game_version_predicate: predicate_or_none(&source.game_versions, source.game_version_scheme.as_deref()),
				loader_version_predicate: None,
				accepted_version_predicate: predicate_or_none(
					&source.accepted_versions,
					source.game_version_scheme.as_deref(),
				),
				qualification,
				declared_by: DeclaredBy {
					kind: declared_by_kind.to_string(),
					id: source.declared_by_id.as_deref().unwrap_or(&accepting_loader).to_string(),
				},
				evidence_digest: None,
				declared_time: now(),
			});
			let signed = sign_payload(ObjectKind::LoaderDef, &acceptance, &[&key]);
			let id = signed.id(ObjectKind::LoaderDef);
			write_object(out, &id, &signed.wire_bytes())?;
			println!("loader_acceptance: {id}");
			String::new()
		}
		other => {
			return Err(format!(
				"unknown `kind = \"{other}\"`; expected game, loader, loader-release, runtime, or mapping"
			));
		}
	};
	if !definition_id.is_empty() {
		let name = source.name.clone().unwrap_or_else(|| default_name.to_string());
		if names.insert(name.clone(), definition_id.clone()).is_some() {
			return Err(format!(
				"`{name}` is defined more than once in this bundle; give each game, loader, and runtime a unique name"
			));
		}
	}
	Ok(definition_id)
}

fn required_field<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, String> {
	value
		.map(str::trim)
		.filter(|text| !text.is_empty())
		.ok_or_else(|| format!("a {name} is required"))
}

fn predicate_or_none(values: &[String], scheme: Option<&str>) -> Option<Predicate> {
	let scheme = scheme.map(str::trim).filter(|text| !text.is_empty()).unwrap_or("exact");
	if values.is_empty() && scheme != "any" {
		return None;
	}
	Some(Predicate {
		scheme: scheme.to_string(),
		values: values.to_vec(),
	})
}

#[cfg(test)]
mod tests;
