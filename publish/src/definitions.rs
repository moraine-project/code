use std::path::Path;

use moraine_crypto::ObjectKind;
use moraine_model::compatibility::{Predicate, Scheme};
use moraine_model::definition::{GameDef, LoaderDef, LoaderObject, LoaderRelease, RuntimeDef, VersionSyntax};
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::signed::sign_payload;

use crate::keyfile;

pub fn game(
	key_path: &Path,
	display_name: &str,
	version_ordering: &str,
	loaders_allowed: bool,
	out: &Path,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let signed_genesis = sign_payload(
		ObjectKind::Genesis,
		&genesis(&key, GenesisKind::Game, &["delegation", "game-def"])?,
		&[&key],
	);
	let game_id = signed_genesis.id(ObjectKind::Genesis);
	let definition = GameDef {
		protocol: 1,
		game_id: game_id.clone(),
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
	};
	let signed_definition = sign_payload(ObjectKind::GameDef, &definition, &[&key]);
	write_pair(out, &game_id, &signed_genesis.wire_bytes(), &signed_definition.wire_bytes())?;
	println!("game_id: {game_id}");
	println!("definition: {}", signed_definition.id(ObjectKind::GameDef));
	Ok(())
}

pub fn loader(key_path: &Path, game_id: &str, display_name: &str, version_ordering: &str, out: &Path) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let signed_genesis = sign_payload(
		ObjectKind::Genesis,
		&genesis(&key, GenesisKind::Loader, &["delegation", "loader-def"])?,
		&[&key],
	);
	let loader_id = signed_genesis.id(ObjectKind::Genesis);
	let definition = LoaderObject::Definition(LoaderDef {
		protocol: 1,
		loader_id: loader_id.clone(),
		game_id: game_id.to_string(),
		display_name: display_name.to_string(),
		version_ordering: version_ordering.to_string(),
		bootstrap: None,
		accepted_artifacts: None,
		declared_time: now(),
	});
	let signed_definition = sign_payload(ObjectKind::LoaderDef, &definition, &[&key]);
	write_pair(out, &loader_id, &signed_genesis.wire_bytes(), &signed_definition.wire_bytes())?;
	println!("loader_id: {loader_id}");
	println!("definition: {}", signed_definition.id(ObjectKind::LoaderDef));
	Ok(())
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

pub fn runtime(key_path: &Path, kind: &str, display_name: &str, version_ordering: &str, out: &Path) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let signed_genesis = sign_payload(
		ObjectKind::Genesis,
		&genesis(&key, GenesisKind::Runtime, &["delegation", "runtime-def"])?,
		&[&key],
	);
	let runtime_id = signed_genesis.id(ObjectKind::Genesis);
	let definition = RuntimeDef {
		protocol: 1,
		runtime_id: runtime_id.clone(),
		kind: kind.to_string(),
		display_name: display_name.to_string(),
		version_ordering: version_ordering.to_string(),
		declared_time: now(),
	};
	let signed_definition = sign_payload(ObjectKind::RuntimeDef, &definition, &[&key]);
	write_pair(
		out,
		&runtime_id,
		&signed_genesis.wire_bytes(),
		&signed_definition.wire_bytes(),
	)?;
	println!("runtime_id: {runtime_id}");
	println!("definition: {}", signed_definition.id(ObjectKind::RuntimeDef));
	Ok(())
}

fn genesis(key: &moraine_crypto::SigningKey, kind: GenesisKind, kinds: &[&str]) -> Result<Genesis, String> {
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
		let definition = moraine_model::definition::GameDef::from_canonical_bytes(
			&moraine_model::signed::SignedObject::<moraine_model::definition::GameDef>::from_bytes(
				&std::fs::read(&definition_path).expect("read"),
			)
			.expect("decode")
			.payload_bytes,
		)
		.expect("game def");
		assert_eq!(definition.game_id, genesis.id);
	}
}
