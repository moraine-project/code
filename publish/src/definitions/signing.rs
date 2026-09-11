use std::path::Path;

use moraine_crypto::{ObjectKind, SigningKey};
use moraine_model::Canonical;
use moraine_model::compatibility::{Predicate, Scheme};
use moraine_model::definition::{
	DeclaredBy, GameDef, LoaderAcceptance, LoaderDef, LoaderObject, LoaderRelease, Qualification, RuntimeDef, VersionSyntax,
};
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::signed::sign_payload;

use super::trimmed;
use crate::keyfile;

#[allow(clippy::too_many_arguments)]
pub fn game(
	key_path: &Path,
	display_name: &str,
	version_ordering: &str,
	versions: &[String],
	loaders_allowed: bool,
	metadata_extractor: Option<&str>,
	install_adapter: Option<&str>,
	revision_of: Option<&str>,
	out: &Path,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let build = |game_id: &str| GameDef {
		protocol: 1,
		game_id: game_id.to_string(),
		display_name: display_name.to_string(),
		version_syntax: VersionSyntax {
			kind: version_ordering.to_string(),
			pattern: None,
		},
		version_ordering: version_ordering.to_string(),
		version_catalog: versions.to_vec(),
		loaders_allowed,
		loader_authorities: Vec::new(),
		categories: Vec::new(),
		tags: Vec::new(),
		metadata_extractor: trimmed(metadata_extractor),
		install_adapter: trimmed(install_adapter),
		declared_time: now(),
	};
	match optional_revision(revision_of) {
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
	};
	Ok(())
}

pub fn loader(
	key_path: &Path,
	game_id: &str,
	display_name: &str,
	version_ordering: &str,
	revision_of: Option<&str>,
	out: &Path,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let build = |loader_id: &str| {
		LoaderObject::Definition(LoaderDef {
			protocol: 1,
			loader_id: loader_id.to_string(),
			game_id: game_id.to_string(),
			display_name: display_name.to_string(),
			version_ordering: version_ordering.to_string(),
			version_catalog: Vec::new(),
			game_versions: None,
			bootstrap: None,
			accepted_artifacts: None,
			declared_time: now(),
		})
	};
	match optional_revision(revision_of) {
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
	};
	Ok(())
}

pub fn runtime(
	key_path: &Path,
	kind: &str,
	display_name: &str,
	version_ordering: &str,
	revision_of: Option<&str>,
	out: &Path,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let build = |runtime_id: &str| RuntimeDef {
		protocol: 1,
		runtime_id: runtime_id.to_string(),
		kind: kind.to_string(),
		display_name: display_name.to_string(),
		version_ordering: version_ordering.to_string(),
		version_catalog: Vec::new(),
		declared_time: now(),
	};
	match optional_revision(revision_of) {
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
	};
	Ok(())
}

pub fn loader_release(
	key_path: &Path,
	loader_id: &str,
	version: &str,
	game_versions: &[String],
	runtime_id: Option<String>,
	runtime_versions: &[String],
	out: &Path,
) -> Result<(), String> {
	if game_versions.is_empty() {
		return Err("at least one --game-version is required".to_string());
	}
	let key = keyfile::load(key_path)?;
	let release = LoaderObject::Release(LoaderRelease {
		protocol: 1,
		loader_id: loader_id.to_string(),
		version_id: version.to_string(),
		game_version_predicate: Predicate::new(Scheme::Exact, game_versions.to_vec()),
		runtime_predicate: (!runtime_versions.is_empty()).then(|| Predicate::new(Scheme::Exact, runtime_versions.to_vec())),
		runtime_id,
		bootstrap: None,
		declared_time: now(),
	});
	let signed = sign_payload(ObjectKind::LoaderDef, &release, &[&key]);
	write_object(out, &signed.id(ObjectKind::LoaderDef), &signed.wire_bytes())?;
	println!("loader_release: {}", signed.id(ObjectKind::LoaderDef));
	Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn loader_acceptance(
	key_path: &Path,
	accepting_loader_id: &str,
	accepted_loader_id: &str,
	game_id: &str,
	qualification: &str,
	game_versions: &[String],
	accepted_versions: &[String],
	declared_by_kind: &str,
	declared_by_id: Option<&str>,
	out: &Path,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let qualification =
		Qualification::parse(qualification).ok_or_else(|| format!("unknown qualification `{qualification}`"))?;
	let acceptance = LoaderObject::Acceptance(LoaderAcceptance {
		protocol: 1,
		accepting_loader_id: accepting_loader_id.to_string(),
		accepted_loader_id: accepted_loader_id.to_string(),
		game_id: game_id.to_string(),
		game_version_predicate: (!game_versions.is_empty()).then(|| Predicate::new(Scheme::Exact, game_versions.to_vec())),
		loader_version_predicate: None,
		accepted_version_predicate: (!accepted_versions.is_empty())
			.then(|| Predicate::new(Scheme::Exact, accepted_versions.to_vec())),
		qualification,
		declared_by: DeclaredBy {
			kind: declared_by_kind.to_string(),
			id: declared_by_id.unwrap_or(accepting_loader_id).to_string(),
		},
		evidence_digest: None,
		declared_time: now(),
	});
	let signed = sign_payload(ObjectKind::LoaderDef, &acceptance, &[&key]);
	write_object(out, &signed.id(ObjectKind::LoaderDef), &signed.wire_bytes())?;
	println!("loader_acceptance: {}", signed.id(ObjectKind::LoaderDef));
	Ok(())
}

pub(super) fn emit<T: Canonical + Clone>(
	key: &SigningKey,
	genesis_kind: GenesisKind,
	authorized: &[&str],
	definition_kind: ObjectKind,
	definition: impl FnOnce(&str) -> T,
	out: &Path,
	label: &str,
) -> Result<String, String> {
	let signed_genesis = sign_payload(ObjectKind::Genesis, &genesis(key, genesis_kind, authorized)?, &[key]);
	let id = signed_genesis.id(ObjectKind::Genesis);
	let signed_definition = sign_payload(definition_kind, &definition(&id), &[key]);
	write_pair(out, &id, &signed_genesis.wire_bytes(), &signed_definition.wire_bytes())?;
	println!("{label}: {id}");
	println!("definition: {}", signed_definition.id(definition_kind));
	Ok(id)
}

fn genesis(key: &SigningKey, kind: GenesisKind, kinds: &[&str]) -> Result<Genesis, String> {
	Ok(Genesis {
		protocol: 1,
		kind,
		nonce: random_nonce(),
		roots: vec![RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).map_err(|error| error.to_string())?],
		threshold: 1,
		authorized_kinds: kinds.iter().map(|kind| kind.to_string()).collect(),
		home_hint: None,
		contacts: None,
		created_at: now(),
	})
}

pub(super) fn optional_revision(value: Option<&str>) -> Option<String> {
	value.map(str::trim).filter(|text| !text.is_empty()).map(str::to_string)
}

pub(super) fn revision_id(value: &str) -> Result<String, String> {
	let trimmed = value.trim();
	let hex = trimmed.strip_prefix("gd:sha256:").unwrap_or_default();
	if hex.len() != 64 || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
		return Err(format!("`{value}` is not a definition id (gd:sha256:<64 hex>)"));
	}
	Ok(trimmed.to_string())
}

pub(super) fn revision<T: Canonical + Clone>(
	key: &SigningKey,
	definition_kind: ObjectKind,
	id: &str,
	definition: impl FnOnce(&str) -> T,
	out: &Path,
	label: &str,
) -> Result<String, String> {
	let signed = sign_payload(definition_kind, &definition(id), &[key]);
	std::fs::create_dir_all(out).map_err(|error| format!("{}: {error}", out.display()))?;
	let stem = id.strip_prefix("gd:sha256:").unwrap_or(id);
	let path = out.join(format!("{stem}.definition"));
	std::fs::write(&path, signed.wire_bytes()).map_err(|error| format!("{}: {error}", path.display()))?;
	println!("{label} revision: {id}");
	println!("definition: {}", signed.id(definition_kind));
	println!("written to {}", out.display());
	Ok(id.to_string())
}

pub(super) fn write_object(directory: &Path, id: &str, object: &[u8]) -> Result<(), String> {
	std::fs::create_dir_all(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
	let stem = id.strip_prefix("gd:sha256:").unwrap_or(id);
	let path = directory.join(format!("{stem}.loader-def"));
	std::fs::write(&path, object).map_err(|error| format!("{}: {error}", path.display()))?;
	println!("written to {}", directory.display());
	Ok(())
}

fn write_pair(directory: &Path, id: &str, genesis: &[u8], definition: &[u8]) -> Result<(), String> {
	std::fs::create_dir_all(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
	let stem = id.strip_prefix("gd:sha256:").unwrap_or(id);
	let genesis_path = directory.join(format!("{stem}.genesis"));
	let definition_path = directory.join(format!("{stem}.definition"));
	std::fs::write(&genesis_path, genesis).map_err(|error| format!("{}: {error}", genesis_path.display()))?;
	std::fs::write(&definition_path, definition).map_err(|error| format!("{}: {error}", definition_path.display()))?;
	println!("written to {}", directory.display());
	Ok(())
}

fn random_nonce() -> Vec<u8> {
	let mut nonce = vec![0u8; 16];
	if getrandom::fill(&mut nonce).is_err() {
		panic!("operating system randomness is unavailable");
	}
	nonce
}

pub(super) fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}
