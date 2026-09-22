use axum::Json;
use axum::body::Bytes;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use moraine_crypto::ObjectKind;
use moraine_model::Canonical;
use moraine_model::definition::{GameDef, LoaderObject, RuntimeDef};
use moraine_model::genesis::{Genesis, GenesisKind};
use moraine_model::signed::SignedObject;
use serde::Serialize;

use super::definitions::{
	DefinitionRow, ensure_game_permits_loader, game_revision_is_forward_only, import_definition, is_loader_definition,
	loader_game_id, loader_names_identity, storage_error, store_failure,
};
use crate::registry::stored;
use crate::routes::AppState;
use crate::verify;

pub(crate) async fn load_directory(state: &AppState, directory: &std::path::Path) -> Result<usize, String> {
	let mut entries = match tokio::fs::read_dir(directory).await {
		Ok(entries) => entries,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
		Err(error) => return Err(error.to_string()),
	};
	let mut files = Vec::new();
	while let Some(entry) = entries.next_entry().await.map_err(|error| error.to_string())? {
		if entry.file_type().await.map_err(|error| error.to_string())?.is_file() {
			files.push(entry.path());
		}
	}
	let mut loaded = 0;
	for path in &files {
		let bytes = tokio::fs::read(path).await.map_err(|error| error.to_string())?;
		let Ok((_, object)) = verify::verify_genesis(&bytes) else {
			continue;
		};
		let Ok(genesis) = Genesis::from_canonical_bytes(&object.payload_bytes) else {
			continue;
		};
		if matches!(genesis.kind, GenesisKind::Project) {
			continue;
		}
		match import_definition(state, genesis.kind, &object).await {
			Ok(()) => loaded += 1,
			Err((_, message)) => return Err(format!("{}: {message}", object.id)),
		}
	}

	let mut versions = Vec::new();
	for path in &files {
		let bytes = tokio::fs::read(path).await.map_err(|error| error.to_string())?;
		if verify::verify_genesis(&bytes).is_ok() {
			continue;
		}
		let Some((expected, kind, id, catalogue)) = definition_target(&bytes) else {
			continue;
		};
		versions.push((expected, kind, id, catalogue, path.clone(), bytes));
	}
	versions.sort_by(|left, right| left.2.cmp(&right.2).then(left.3.cmp(&right.3)).then(left.4.cmp(&right.4)));
	for (expected, kind, id, _, _, bytes) in versions {
		match store_definition_version(state, expected, kind, &id, &bytes).await {
			Ok(_) => loaded += 1,
			Err((_, message)) => return Err(format!("{id}: {message}")),
		}
	}
	Ok(loaded)
}

fn definition_target(bytes: &[u8]) -> Option<(GenesisKind, ObjectKind, String, usize)> {
	if let Ok(signed) = SignedObject::<GameDef>::from_bytes(bytes) {
		return Some((
			GenesisKind::Game,
			ObjectKind::GameDef,
			signed.payload.game_id,
			signed.payload.version_catalog.len(),
		));
	}
	if let Ok(signed) = SignedObject::<LoaderObject>::from_bytes(bytes) {
		let (loader_id, catalogue) = match &signed.payload {
			LoaderObject::Definition(definition) => (definition.loader_id.clone(), definition.version_catalog.len()),
			LoaderObject::Release(release) => (release.loader_id.clone(), 0),
			LoaderObject::Acceptance(acceptance) => (acceptance.accepting_loader_id.clone(), 0),
		};
		return Some((GenesisKind::Loader, ObjectKind::LoaderDef, loader_id, catalogue));
	}
	if let Ok(signed) = SignedObject::<RuntimeDef>::from_bytes(bytes) {
		return Some((
			GenesisKind::Runtime,
			ObjectKind::RuntimeDef,
			signed.payload.runtime_id,
			signed.payload.version_catalog.len(),
		));
	}
	None
}

#[derive(Serialize)]
struct DefinitionView {
	id: String,
	kind: &'static str,
	genesis: String,
	current: String,
	source_home: Option<String>,
	payload: serde_json::Value,
}

pub(crate) async fn put_definition(
	state: &AppState,
	expected: GenesisKind,
	kind: ObjectKind,
	id: &str,
	body: Bytes,
) -> Response {
	match store_definition_version(state, expected, kind, id, &body).await {
		Ok(object_id) => (StatusCode::CREATED, Json(serde_json::json!({ "definition": object_id }))).into_response(),
		Err((status, message)) => (status, message).into_response(),
	}
}

async fn store_definition_version(
	state: &AppState,
	expected: GenesisKind,
	kind: ObjectKind,
	id: &str,
	body: &[u8],
) -> Result<String, (StatusCode, String)> {
	let definition = match state.metadata.definition(id).await.map_err(store_failure)? {
		Some(definition) => definition,
		None => return Err((StatusCode::NOT_FOUND, format!("no such {} definition", expected.as_str()))),
	};
	if definition.kind != expected.as_str() {
		return Err((StatusCode::BAD_REQUEST, format!("`{id}` is not a {}", expected.as_str())));
	}
	let Some(genesis) = state
		.metadata
		.object(&definition.genesis_digest)
		.await
		.map_err(store_failure)?
	else {
		return Err((StatusCode::INTERNAL_SERVER_ERROR, "definition genesis is missing".to_string()));
	};
	let (root, _) =
		verify::verify_genesis(&genesis.wire).map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
	let object = verify::verify_object(kind, body, &root).map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
	if expected == GenesisKind::Game
		&& let Ok(next) = GameDef::from_canonical_bytes(&object.payload_bytes)
	{
		let previous = match definition.current_digest.as_deref() {
			Some(digest) => state
				.metadata
				.object(digest)
				.await
				.map_err(store_failure)?
				.and_then(|object| GameDef::from_canonical_bytes(&object.payload).ok()),
			None => None,
		};
		if let Err(message) = game_revision_is_forward_only(id, previous.as_ref(), &next) {
			return Err((StatusCode::BAD_REQUEST, message));
		}
	}
	if expected == GenesisKind::Loader
		&& let Some(game_id) = loader_game_id(&object.payload_bytes)
	{
		ensure_game_permits_loader(state, id, &game_id).await?;
	}
	if expected == GenesisKind::Loader
		&& let Err(message) = loader_names_identity(&object.payload_bytes, id)
	{
		return Err((StatusCode::BAD_REQUEST, message));
	}
	if expected == GenesisKind::Runtime
		&& let Ok(definition) = RuntimeDef::from_canonical_bytes(&object.payload_bytes)
		&& definition.runtime_id != id
	{
		return Err((
			StatusCode::BAD_REQUEST,
			"the definition names a different runtime than the identity it is stored under".to_string(),
		));
	}
	state.metadata.put_object(&stored(&object)).await.map_err(store_failure)?;
	if expected == GenesisKind::Loader
		&& let Ok(loader_object) = LoaderObject::from_canonical_bytes(&object.payload_bytes)
	{
		let indexed = match &loader_object {
			LoaderObject::Release(release) => {
				state
					.metadata
					.index_loader_release(&release.loader_id, &release.version_id, &object.digest, release.declared_time)
					.await
			}
			LoaderObject::Acceptance(acceptance) => state.metadata.index_loader_acceptance(acceptance, &object.digest).await,
			LoaderObject::Definition(_) => Ok(()),
		};
		if let Err(error) = indexed {
			if let sqlx::Error::Protocol(message) = &error {
				return Err((StatusCode::CONFLICT, message.clone()));
			}
			return Err(store_failure(error));
		}
	}
	if expected != GenesisKind::Loader || is_loader_definition(&object.payload_bytes) {
		state
			.metadata
			.set_definition_current(id, &object.digest)
			.await
			.map_err(store_failure)?;
	}
	Ok(object.id)
}

pub(crate) async fn get_definition(state: &AppState, expected: GenesisKind, id: &str) -> Response {
	let definition = match load_definition(state, expected, id).await {
		Ok(definition) => definition,
		Err(response) => return *response,
	};
	let Some(current) = definition.current_digest else {
		return (StatusCode::NOT_FOUND, "no definition published yet").into_response();
	};
	let Some(object) = (match state.metadata.object(&current).await {
		Ok(object) => object,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "definition object is missing").into_response();
	};
	let payload = match crate::registry::views::payload_json(&object.payload) {
		Ok(payload) => payload,
		Err(error) => return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
	};
	let view = DefinitionView {
		id: definition.id,
		kind: expected.as_str(),
		genesis: id_for(&definition.genesis_digest),
		current: id_for(&current),
		source_home: definition.source_home,
		payload,
	};
	Json(view).into_response()
}

pub(crate) async fn load_definition(
	state: &AppState,
	expected: GenesisKind,
	id: &str,
) -> Result<DefinitionRow, Box<Response>> {
	match state.metadata.definition(id).await {
		Ok(Some(definition)) if definition.kind == expected.as_str() => Ok(definition),
		Ok(Some(_)) => Err(Box::new(
			(StatusCode::BAD_REQUEST, "id is not that kind of definition").into_response(),
		)),
		Ok(None) => Err(Box::new((StatusCode::NOT_FOUND, "no such definition").into_response())),
		Err(error) => Err(Box::new(storage_error(error))),
	}
}

pub(crate) fn id_for(digest: &[u8]) -> String {
	format!("gd:sha256:{}", hex::encode(digest))
}
