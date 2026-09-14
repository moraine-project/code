use std::path::Path;

use moraine_model::Canonical;
use moraine_model::definition::{GameDef, LoaderObject, RuntimeDef};
use moraine_model::genesis::{Genesis, GenesisKind};
use moraine_model::signed::SignedObject;

use crate::home::Home;

enum Target {
	Genesis(GenesisKind),
	Definition(GenesisKind, String),
}

fn classify(bytes: &[u8]) -> Option<Target> {
	if let Ok((_, object)) = moraine_model::verify::verify_genesis(bytes) {
		let genesis = Genesis::from_canonical_bytes(&object.payload_bytes).ok()?;
		return Some(Target::Genesis(genesis.kind));
	}
	if let Ok(signed) = SignedObject::<GameDef>::from_bytes(bytes) {
		return Some(Target::Definition(GenesisKind::Game, signed.payload.game_id));
	}
	if let Ok(signed) = SignedObject::<LoaderObject>::from_bytes(bytes) {
		let id = match &signed.payload {
			LoaderObject::Definition(definition) => definition.loader_id.clone(),
			LoaderObject::Release(release) => release.loader_id.clone(),
			LoaderObject::Acceptance(acceptance) => acceptance.accepting_loader_id.clone(),
		};
		return Some(Target::Definition(GenesisKind::Loader, id));
	}
	if let Ok(signed) = SignedObject::<RuntimeDef>::from_bytes(bytes) {
		return Some(Target::Definition(GenesisKind::Runtime, signed.payload.runtime_id));
	}
	None
}

fn collection(kind: GenesisKind) -> &'static str {
	match kind {
		GenesisKind::Game => "games",
		GenesisKind::Loader => "loaders",
		GenesisKind::Runtime => "runtimes",
		GenesisKind::Project => "projects",
	}
}

pub async fn push(home_url: &str, directory: &Path, api_key: Option<&str>) -> Result<usize, String> {
	let home = Home::new(home_url)?;
	let mut paths = std::fs::read_dir(directory)
		.map_err(|error| format!("{}: {error}", directory.display()))?
		.filter_map(|entry| entry.ok().map(|entry| entry.path()))
		.filter(|path| path.is_file())
		.collect::<Vec<_>>();
	paths.sort();

	let mut genesis = Vec::new();
	let mut definitions = Vec::new();
	for path in paths {
		let bytes = std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
		match classify(&bytes) {
			Some(Target::Genesis(kind)) => genesis.push((kind, bytes)),
			Some(Target::Definition(kind, id)) => definitions.push((kind, id, bytes)),
			None => {}
		}
	}
	definitions.sort_by_key(|(kind, ..)| match kind {
		GenesisKind::Game | GenesisKind::Runtime => 0,
		GenesisKind::Loader => 1,
		GenesisKind::Project => 2,
	});

	let mut imported = 0;
	for (kind, bytes) in genesis {
		home.post_wire_with_token(&format!("/v1/{}", collection(kind)), bytes, api_key)
			.await?;
		imported += 1;
	}
	for (kind, id, bytes) in definitions {
		home.post_wire_with_token(&format!("/v1/{}/{id}/definitions", collection(kind)), bytes, api_key)
			.await?;
		imported += 1;
	}
	println!("imported {imported} definition object(s) into {home_url}");
	Ok(imported)
}
