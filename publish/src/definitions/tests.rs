use moraine_model::Canonical;

use super::*;

#[test]
fn writes_a_game_genesis_and_definition_pair() {
	let directory = tempfile::tempdir().expect("tempdir");
	let key_path = directory.path().join("game.key");
	keyfile::create(&key_path).expect("key");
	let out = directory.path().join("definitions");

	game(
		&key_path,
		"Example Game",
		"semver",
		&[],
		true,
		None,
		Some("sims4/default"),
		None,
		&out,
	)
	.expect("game");

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
	game(
		&key_path,
		"Example Game",
		"semver",
		&[],
		true,
		None,
		Some("sims4/default"),
		None,
		&out,
	)
	.expect("game");

	let genesis_path = std::fs::read_dir(&out)
		.expect("read")
		.filter_map(|entry| Some(entry.ok()?.path()))
		.find(|path| path.extension().is_some_and(|extension| extension == "genesis"))
		.expect("genesis file");
	let definition_path = genesis_path.with_extension("definition");
	let (_, genesis) = moraine_model::verify::verify_genesis(&std::fs::read(&genesis_path).expect("read")).expect("genesis");
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
version_ordering = "ordered-list"
loaders_allowed = true
versions = ["1.20", "1.20.1", "1.21"]

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
	assert_eq!(definition.version_catalog, vec!["1.20", "1.20.1", "1.21"]);
}

#[test]
fn reads_the_named_extractor_and_adapter_from_a_file() {
	let directory = tempfile::tempdir().expect("tempdir");
	let key_path = directory.path().join("game.key");
	keyfile::create(&key_path).expect("key");
	let source = directory.path().join("sims4.toml");
	std::fs::write(
		&source,
		r#"kind = "game"
display_name = "The Sims 4"
version_ordering = "opaque"
loaders_allowed = false
metadata_extractor = "  "
install_adapter = "sims4/default"
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
	assert_eq!(definition.install_adapter.as_deref(), Some("sims4/default"));
	assert_eq!(definition.metadata_extractor, None, "a blank value means unset");
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
fn reads_an_acceptance_mapping_from_a_readable_file() {
	use moraine_model::signed::SignedObject;

	let directory = tempfile::tempdir().expect("tempdir");
	let key_path = directory.path().join("loader.key");
	keyfile::create(&key_path).expect("key");
	let source = directory.path().join("cleanroom-accepts-forge.toml");
	std::fs::write(
		&source,
		r#"kind = "mapping"
accepting_loader = "gd:sha256:accepting"
accepted_loader = "gd:sha256:accepted"
game_id = "gd:sha256:game"
qualification = "most"
declared_by = "loader-authority"
"#,
	)
	.expect("write");
	let out = directory.path().join("definitions");

	from_file(&key_path, &source, &out).expect("compile");

	let path = std::fs::read_dir(&out)
		.expect("read")
		.filter_map(|entry| Some(entry.ok()?.path()))
		.find(|path| path.extension().is_some_and(|extension| extension == "loader-def"))
		.expect("mapping file");
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
fn reads_a_loader_release_from_a_readable_file() {
	use moraine_model::signed::SignedObject;

	let directory = tempfile::tempdir().expect("tempdir");
	let key_path = directory.path().join("loader.key");
	keyfile::create(&key_path).expect("key");
	let source = directory.path().join("fabric-0.15.0.toml");
	std::fs::write(
		&source,
		r#"kind = "loader-release"
loader_id = "gd:sha256:loader"
version = "0.15.0"
game_versions = ["1.20.1"]
runtime_id = "gd:sha256:runtime"
runtime_versions = ["17"]
"#,
	)
	.expect("write");
	let out = directory.path().join("definitions");

	from_file(&key_path, &source, &out).expect("compile");

	let path = std::fs::read_dir(&out)
		.expect("read")
		.filter_map(|entry| Some(entry.ok()?.path()))
		.find(|path| path.extension().is_some_and(|extension| extension == "loader-def"))
		.expect("release file");
	let signed = SignedObject::<LoaderObject>::from_bytes(&std::fs::read(&path).expect("read")).expect("decode");
	let LoaderObject::Release(release) = signed.payload else {
		panic!("expected a loader release");
	};
	assert_eq!(release.loader_id, "gd:sha256:loader");
	assert_eq!(release.version_id, "0.15.0");
	assert_eq!(release.runtime_id.as_deref(), Some("gd:sha256:runtime"));
	assert_eq!(release.runtime_predicate.expect("runtime predicate").values, vec!["17"]);
}

#[test]
fn reads_a_loader_version_table_from_a_file() {
	use moraine_model::signed::SignedObject;

	let directory = tempfile::tempdir().expect("tempdir");
	let key_path = directory.path().join("loader.key");
	keyfile::create(&key_path).expect("key");
	let source = directory.path().join("neoforge.toml");
	std::fs::write(
		&source,
		r#"kind = "loader"
display_name = "NeoForge"
version_ordering = "ordered-list"
game_id = "gd:sha256:game"
versions = ["20.4.237", "21.1.72", "26.3.0.7-beta"]
game_versions = ["1.20.4", "1.21", "26.3"]
"#,
	)
	.expect("write");
	let out = directory.path().join("definitions");

	from_file(&key_path, &source, &out).expect("compile");

	let path = std::fs::read_dir(&out)
		.expect("read")
		.filter_map(|entry| Some(entry.ok()?.path()))
		.find(|path| path.extension().is_some_and(|extension| extension == "definition"))
		.expect("definition file");
	let signed = SignedObject::<LoaderObject>::from_bytes(&std::fs::read(&path).expect("read")).expect("decode");
	let LoaderObject::Definition(definition) = signed.payload else {
		panic!("expected a loader definition");
	};
	assert_eq!(definition.version_ordering, "ordered-list");
	assert_eq!(definition.version_catalog, vec!["20.4.237", "21.1.72", "26.3.0.7-beta"]);
	assert!(definition.catalog().is_some());
}

#[test]
fn compiles_a_bundle_resolving_names() {
	use moraine_model::genesis::Genesis;
	use moraine_model::signed::SignedObject;

	let directory = tempfile::tempdir().expect("tempdir");
	let key_path = directory.path().join("bundle.key");
	keyfile::create(&key_path).expect("key");
	let source = directory.path().join("bundle");
	std::fs::create_dir_all(source.join("loaders/fabric")).expect("mkdir");
	std::fs::write(
		source.join("game.toml"),
		"kind = \"game\"\nname = \"minecraft\"\ndisplay_name = \"Minecraft\"\nversion_ordering = \"ordered-list\"\nversions = [\"1.20.1\"]\n",
	)
	.expect("write");
	std::fs::write(
		source.join("loaders/fabric.toml"),
		"kind = \"loader\"\nname = \"fabric\"\ndisplay_name = \"Fabric\"\nversion_ordering = \"semver\"\ngame_id = \"minecraft\"\n",
	)
	.expect("write");
	std::fs::write(
		source.join("loaders/fabric/0.15.0.toml"),
		"kind = \"loader-release\"\nloader_id = \"fabric\"\nversion = \"0.15.0\"\ngame_versions = [\"1.20.1\"]\n",
	)
	.expect("write");
	let out = directory.path().join("out");

	from_directory(&key_path, &source, &out).expect("compile");

	let mut game_id = None;
	let mut loader_game = None;
	for entry in std::fs::read_dir(&out).expect("read") {
		let path = entry.expect("entry").path();
		if path.extension().is_some_and(|extension| extension == "genesis") {
			let (_, object) = moraine_model::verify::verify_genesis(&std::fs::read(&path).expect("read")).expect("verify");
			let genesis = Genesis::from_canonical_bytes(&object.payload_bytes).expect("genesis");
			if genesis.kind == GenesisKind::Game {
				game_id = Some(object.id);
			}
		}
		if path.extension().is_some_and(|extension| extension == "definition")
			&& let Ok(signed) = SignedObject::<LoaderObject>::from_bytes(&std::fs::read(&path).expect("read"))
			&& let LoaderObject::Definition(definition) = signed.payload
		{
			loader_game = Some(definition.game_id);
		}
	}
	assert_eq!(loader_game, game_id, "the loader's game_id resolved to the game's identity");

	let releases = std::fs::read_dir(&out)
		.expect("read")
		.filter(|entry| {
			entry
				.as_ref()
				.is_ok_and(|entry| entry.path().extension().is_some_and(|e| e == "loader-def"))
		})
		.count();
	assert_eq!(releases, 1, "the loader release was written");
}

#[test]
fn reads_a_loader_family_game_version_range_from_a_file() {
	use moraine_model::signed::SignedObject;

	let directory = tempfile::tempdir().expect("tempdir");
	let key_path = directory.path().join("loader.key");
	keyfile::create(&key_path).expect("key");
	let source = directory.path().join("fabric.toml");
	std::fs::write(
		&source,
		r#"kind = "loader"
display_name = "Fabric"
version_ordering = "semver"
game_id = "gd:sha256:game"
game_version_scheme = "ordered-list"
game_versions = ["1.14..=26.3"]
"#,
	)
	.expect("write");
	let out = directory.path().join("definitions");

	from_file(&key_path, &source, &out).expect("compile");

	let path = std::fs::read_dir(&out)
		.expect("read")
		.filter_map(|entry| Some(entry.ok()?.path()))
		.find(|path| path.extension().is_some_and(|extension| extension == "definition"))
		.expect("definition file");
	let signed = SignedObject::<LoaderObject>::from_bytes(&std::fs::read(&path).expect("read")).expect("decode");
	let LoaderObject::Definition(definition) = signed.payload else {
		panic!("expected a loader definition");
	};
	let game_versions = definition.game_versions.expect("family game versions");
	assert_eq!(game_versions.scheme, "ordered-list");
	assert_eq!(game_versions.values, vec!["1.14..=26.3"]);
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

fn genesis_stem(out: &Path) -> String {
	std::fs::read_dir(out)
		.expect("read")
		.filter_map(|entry| Some(entry.ok()?.path()))
		.find(|path| path.extension().is_some_and(|extension| extension == "genesis"))
		.expect("genesis file")
		.file_stem()
		.expect("stem")
		.to_string_lossy()
		.to_string()
}

#[test]
fn compiles_a_game_revision_under_the_same_identity() {
	use moraine_model::signed::SignedObject;

	let directory = tempfile::tempdir().expect("tempdir");
	let key_path = directory.path().join("game.key");
	keyfile::create(&key_path).expect("key");
	let out = directory.path().join("definitions");

	game(
		&key_path,
		"Minecraft",
		"ordered-list",
		&["1.20.1".to_string()],
		true,
		None,
		None,
		None,
		&out,
	)
	.expect("game");
	let stem = genesis_stem(&out);
	let id = format!("gd:sha256:{stem}");

	game(
		&key_path,
		"Minecraft",
		"ordered-list",
		&["1.20.1".to_string(), "1.22".to_string()],
		true,
		None,
		None,
		Some(&id),
		&out,
	)
	.expect("revision");

	let signed = SignedObject::<GameDef>::from_bytes(&std::fs::read(out.join(format!("{stem}.definition"))).expect("read"))
		.expect("decode");
	assert_eq!(signed.payload.game_id, id, "a revision keeps the identity");
	assert_eq!(signed.payload.version_catalog, vec!["1.20.1", "1.22"]);

	let genesis_count = std::fs::read_dir(&out)
		.expect("read")
		.filter(|entry| {
			entry
				.as_ref()
				.is_ok_and(|entry| entry.path().extension().is_some_and(|e| e == "genesis"))
		})
		.count();
	assert_eq!(genesis_count, 1, "a revision does not create a new genesis");
}

#[test]
fn reads_a_game_revision_from_a_readable_file() {
	use moraine_model::signed::SignedObject;

	let directory = tempfile::tempdir().expect("tempdir");
	let key_path = directory.path().join("game.key");
	keyfile::create(&key_path).expect("key");
	let out = directory.path().join("definitions");

	game(
		&key_path,
		"Minecraft",
		"ordered-list",
		&["1.20.1".to_string()],
		true,
		None,
		None,
		None,
		&out,
	)
	.expect("game");
	let stem = genesis_stem(&out);
	let id = format!("gd:sha256:{stem}");

	let source = directory.path().join("minecraft.toml");
	std::fs::write(
		&source,
		format!(
			r#"kind = "game"
revision_of = "{id}"
display_name = "Minecraft"
version_ordering = "ordered-list"
versions = ["1.20.1", "1.22"]
"#
		),
	)
	.expect("write");

	from_file(&key_path, &source, &out).expect("compile");

	let signed = SignedObject::<GameDef>::from_bytes(&std::fs::read(out.join(format!("{stem}.definition"))).expect("read"))
		.expect("decode");
	assert_eq!(signed.payload.game_id, id);
	assert_eq!(signed.payload.version_catalog, vec!["1.20.1", "1.22"]);
}

#[test]
fn rejects_a_revision_of_a_non_identity_object() {
	let directory = tempfile::tempdir().expect("tempdir");
	let key_path = directory.path().join("key");
	keyfile::create(&key_path).expect("key");
	let source = directory.path().join("release.toml");
	std::fs::write(
		&source,
		r#"kind = "loader-release"
revision_of = "gd:sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
loader_id = "gd:sha256:loader"
version = "1.0.0"
game_versions = ["1.20.1"]
"#,
	)
	.expect("write");
	let error = from_file(&key_path, &source, &directory.path().join("out")).expect_err("error");
	assert!(error.contains("revision_of"), "{error}");
}

#[test]
fn rejects_a_revision_with_a_malformed_identity() {
	let directory = tempfile::tempdir().expect("tempdir");
	let key_path = directory.path().join("key");
	keyfile::create(&key_path).expect("key");
	let source = directory.path().join("game.toml");
	std::fs::write(
		&source,
		r#"kind = "game"
revision_of = "minecraft"
display_name = "Minecraft"
version_ordering = "ordered-list"
"#,
	)
	.expect("write");
	let error = from_file(&key_path, &source, &directory.path().join("out")).expect_err("error");
	assert!(error.contains("not a definition id"), "{error}");
}
