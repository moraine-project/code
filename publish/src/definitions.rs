use std::path::Path;

use moraine_crypto::{ObjectKind, SigningKey};
use moraine_model::Canonical;
use moraine_model::compatibility::{Predicate, Scheme};
use moraine_model::definition::{
	Category as GameCategory, DeclaredBy, GameDef, LoaderAcceptance, LoaderDef, LoaderObject, LoaderRelease, Qualification,
	RuntimeDef, Tag as GameTag, VersionSyntax,
};
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::signed::sign_payload;
use serde::Deserialize;

use crate::keyfile;

pub fn game(
	key_path: &Path,
	display_name: &str,
	version_ordering: &str,
	loaders_allowed: bool,
	out: &Path,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	emit(
		&key,
		GenesisKind::Game,
		&["delegation", "game-def"],
		ObjectKind::GameDef,
		|game_id| GameDef {
			protocol: 1,
			game_id: game_id.to_string(),
			display_name: display_name.to_string(),
			version_syntax: VersionSyntax {
				kind: version_ordering.to_string(),
				pattern: None,
			},
			version_ordering: version_ordering.to_string(),
			loaders_allowed,
			loader_authorities: Vec::new(),
			categories: Vec::new(),
			tags: Vec::new(),
			metadata_extractor: None,
			install_adapter: None,
			declared_time: now(),
		},
		out,
		"game_id",
	)
}

pub fn loader(key_path: &Path, game_id: &str, display_name: &str, version_ordering: &str, out: &Path) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	emit(
		&key,
		GenesisKind::Loader,
		&["delegation", "loader-def"],
		ObjectKind::LoaderDef,
		|loader_id| {
			LoaderObject::Definition(LoaderDef {
				protocol: 1,
				loader_id: loader_id.to_string(),
				game_id: game_id.to_string(),
				display_name: display_name.to_string(),
				version_ordering: version_ordering.to_string(),
				bootstrap: None,
				accepted_artifacts: None,
				declared_time: now(),
			})
		},
		out,
		"loader_id",
	)
}

pub fn runtime(key_path: &Path, kind: &str, display_name: &str, version_ordering: &str, out: &Path) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	emit(
		&key,
		GenesisKind::Runtime,
		&["delegation", "runtime-def"],
		ObjectKind::RuntimeDef,
		|runtime_id| RuntimeDef {
			protocol: 1,
			runtime_id: runtime_id.to_string(),
			kind: kind.to_string(),
			display_name: display_name.to_string(),
			version_ordering: version_ordering.to_string(),
			declared_time: now(),
		},
		out,
		"runtime_id",
	)
}

#[derive(Deserialize)]
struct DefinitionFile {
	kind: String,
	display_name: String,
	version_ordering: String,
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
	runtime_kind: Option<String>,
}

#[derive(Deserialize)]
struct CategoryFile {
	id: String,
	label: String,
	#[serde(default)]
	parent: Option<String>,
}

#[derive(Deserialize)]
struct TagFile {
	id: String,
	label: String,
}

pub fn from_file(key_path: &Path, file: &Path, out: &Path) -> Result<(), String> {
	let text = std::fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
	let source: DefinitionFile = toml::from_str(&text).map_err(|error| format!("{}: {error}", file.display()))?;
	let key = keyfile::load(key_path)?;
	let display_name = source.display_name.trim().to_string();
	let ordering = source.version_ordering.trim().to_string();
	if display_name.is_empty() || ordering.is_empty() {
		return Err("display_name and version_ordering are required".to_string());
	}
	match source.kind.as_str() {
		"game" => emit(
			&key,
			GenesisKind::Game,
			&["delegation", "game-def"],
			ObjectKind::GameDef,
			|game_id| GameDef {
				protocol: 1,
				game_id: game_id.to_string(),
				display_name: display_name.clone(),
				version_syntax: VersionSyntax {
					kind: ordering.clone(),
					pattern: None,
				},
				version_ordering: ordering.clone(),
				loaders_allowed: source.loaders_allowed.unwrap_or(true),
				loader_authorities: source.loader_authorities.clone(),
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
				metadata_extractor: None,
				install_adapter: None,
				declared_time: now(),
			},
			out,
			"game_id",
		),
		"loader" => {
			let game_id = source
				.game_id
				.as_deref()
				.map(str::trim)
				.filter(|value| !value.is_empty())
				.ok_or_else(|| "a loader definition needs game_id".to_string())?;
			emit(
				&key,
				GenesisKind::Loader,
				&["delegation", "loader-def"],
				ObjectKind::LoaderDef,
				|loader_id| {
					LoaderObject::Definition(LoaderDef {
						protocol: 1,
						loader_id: loader_id.to_string(),
						game_id: game_id.to_string(),
						display_name: display_name.clone(),
						version_ordering: ordering.clone(),
						bootstrap: None,
						accepted_artifacts: None,
						declared_time: now(),
					})
				},
				out,
				"loader_id",
			)
		}
		"runtime" => {
			let runtime_kind = source.runtime_kind.as_deref().unwrap_or("java");
			emit(
				&key,
				GenesisKind::Runtime,
				&["delegation", "runtime-def"],
				ObjectKind::RuntimeDef,
				|runtime_id| RuntimeDef {
					protocol: 1,
					runtime_id: runtime_id.to_string(),
					kind: runtime_kind.to_string(),
					display_name: display_name.clone(),
					version_ordering: ordering.clone(),
					declared_time: now(),
				},
				out,
				"runtime_id",
			)
		}
		other => Err(format!("unknown `kind = \"{other}\"`; expected game, loader, or runtime")),
	}
}

#[allow(clippy::too_many_arguments)]
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

fn emit<T: Canonical + Clone>(
	key: &SigningKey,
	genesis_kind: GenesisKind,
	authorized: &[&str],
	definition_kind: ObjectKind,
	definition: impl FnOnce(&str) -> T,
	out: &Path,
	label: &str,
) -> Result<(), String> {
	let signed_genesis = sign_payload(ObjectKind::Genesis, &genesis(key, genesis_kind, authorized)?, &[key]);
	let id = signed_genesis.id(ObjectKind::Genesis);
	let signed_definition = sign_payload(definition_kind, &definition(&id), &[key]);
	write_pair(out, &id, &signed_genesis.wire_bytes(), &signed_definition.wire_bytes())?;
	println!("{label}: {id}");
	println!("definition: {}", signed_definition.id(definition_kind));
	Ok(())
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

fn write_object(directory: &Path, id: &str, object: &[u8]) -> Result<(), String> {
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

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

#[cfg(test)]
mod tests {
	use moraine_model::Canonical;

	use super::*;

	#[test]
	fn writes_a_game_genesis_and_definition_pair() {
		let directory = tempfile::tempdir().expect("tempdir");
		let key_path = directory.path().join("game.key");
		keyfile::create(&key_path).expect("key");
		let out = directory.path().join("definitions");

		game(&key_path, "Example Game", "semver", true, &out).expect("game");

		let files: Vec<String> = std::fs::read_dir(&out)
			.expect("read")
			.filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().to_string()))
			.collect();
		assert_eq!(files.len(), 2);
		assert!(files.iter().any(|name| name.ends_with(".genesis")));
		assert!(files.iter().any(|name| name.ends_with(".definition")));
	}

	#[test]
	fn writes_a_loader_release_referencing_the_given_loader() {
		use moraine_model::signed::SignedObject;

		let directory = tempfile::tempdir().expect("tempdir");
		let key_path = directory.path().join("loader.key");
		keyfile::create(&key_path).expect("key");
		let out = directory.path().join("definitions");
		loader_release(
			&key_path,
			"gd:sha256:ab",
			"0.15.0",
			&["1.20.1".to_string()],
			Some("gd:sha256:cd".to_string()),
			&["17".to_string()],
			&out,
		)
		.expect("loader release");

		let path = std::fs::read_dir(&out)
			.expect("read")
			.filter_map(|entry| Some(entry.ok()?.path()))
			.find(|path| path.extension().is_some_and(|extension| extension == "loader-def"))
			.expect("loader release file");
		let signed = SignedObject::<LoaderObject>::from_bytes(&std::fs::read(&path).expect("read")).expect("decode");
		let LoaderObject::Release(release) = signed.payload else {
			panic!("expected a loader release");
		};
		assert_eq!(release.loader_id, "gd:sha256:ab");
		assert_eq!(release.version_id, "0.15.0");
		assert_eq!(release.runtime_id.as_deref(), Some("gd:sha256:cd"));
	}

	#[test]
	fn the_definition_id_matches_the_genesis_id() {
		let directory = tempfile::tempdir().expect("tempdir");
		let key_path = directory.path().join("game.key");
		keyfile::create(&key_path).expect("key");
		let out = directory.path().join("definitions");
		game(&key_path, "Example Game", "semver", true, &out).expect("game");

		let genesis_path = std::fs::read_dir(&out)
			.expect("read")
			.filter_map(|entry| Some(entry.ok()?.path()))
			.find(|path| path.extension().is_some_and(|extension| extension == "genesis"))
			.expect("genesis file");
		let definition_path = genesis_path.with_extension("definition");
		let (_, genesis) =
			moraine_model::verify::verify_genesis(&std::fs::read(&genesis_path).expect("read")).expect("genesis");
		let definition = GameDef::from_canonical_bytes(
			&moraine_model::signed::SignedObject::<GameDef>::from_bytes(&std::fs::read(&definition_path).expect("read"))
				.expect("decode")
				.payload_bytes,
		)
		.expect("game def");
		assert_eq!(definition.game_id, genesis.id);
	}

	#[test]
	fn reads_a_game_from_a_readable_file() {
		let directory = tempfile::tempdir().expect("tempdir");
		let key_path = directory.path().join("game.key");
		keyfile::create(&key_path).expect("key");
		let source = directory.path().join("minecraft.toml");
		std::fs::write(
			&source,
			r#"kind = "game"
display_name = "Minecraft"
version_ordering = "semver"
loaders_allowed = true

[[categories]]
id = "utility"
label = "Utility"

[[tags]]
id = "client"
label = "Client"
"#,
		)
		.expect("write");
		let out = directory.path().join("definitions");

		from_file(&key_path, &source, &out).expect("compile");

		let definition_path = std::fs::read_dir(&out)
			.expect("read")
			.filter_map(|entry| Some(entry.ok()?.path()))
			.find(|path| path.extension().is_some_and(|extension| extension == "definition"))
			.expect("definition file");
		let definition = GameDef::from_canonical_bytes(
			&moraine_model::signed::SignedObject::<GameDef>::from_bytes(&std::fs::read(&definition_path).expect("read"))
				.expect("decode")
				.payload_bytes,
		)
		.expect("game def");
		assert_eq!(definition.display_name, "Minecraft");
		assert_eq!(definition.categories[0].id, "utility");
		assert_eq!(definition.tags[0].label, "Client");
	}

	#[test]
	fn writes_an_acceptance_mapping() {
		use moraine_model::signed::SignedObject;

		let directory = tempfile::tempdir().expect("tempdir");
		let key_path = directory.path().join("loader.key");
		keyfile::create(&key_path).expect("key");
		let out = directory.path().join("definitions");
		loader_acceptance(
			&key_path,
			"gd:sha256:accepting",
			"gd:sha256:accepted",
			"gd:sha256:game",
			"most",
			&["1.20.1".to_string()],
			&[],
			"loader-authority",
			None,
			&out,
		)
		.expect("acceptance");

		let path = std::fs::read_dir(&out)
			.expect("read")
			.filter_map(|entry| Some(entry.ok()?.path()))
			.next()
			.expect("file");
		let signed = SignedObject::<LoaderObject>::from_bytes(&std::fs::read(&path).expect("read")).expect("decode");
		let LoaderObject::Acceptance(acceptance) = signed.payload else {
			panic!("expected an acceptance mapping");
		};
		assert_eq!(acceptance.accepting_loader_id, "gd:sha256:accepting");
		assert_eq!(acceptance.accepted_loader_id, "gd:sha256:accepted");
		assert_eq!(acceptance.qualification.as_str(), "most");
		assert_eq!(acceptance.declared_by.id, "gd:sha256:accepting");
	}

	#[test]
	fn rejects_a_file_with_an_unknown_kind() {
		let directory = tempfile::tempdir().expect("tempdir");
		let key_path = directory.path().join("key");
		keyfile::create(&key_path).expect("key");
		let source = directory.path().join("bad.toml");
		std::fs::write(
			&source,
			"kind = \"widget\"\ndisplay_name = \"X\"\nversion_ordering = \"semver\"\n",
		)
		.expect("write");
		let error = from_file(&key_path, &source, &directory.path().join("out")).expect_err("error");
		assert!(error.contains("unknown `kind"));
	}
}
