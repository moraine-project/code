use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_crypto::ObjectKind;
use moraine_model::Canonical;
use moraine_model::definition::{GameDef, LoaderObject, RuntimeDef};
use moraine_model::genesis::{Genesis, GenesisKind};
use moraine_model::signed::SignedObject;
use serde::Serialize;
use sqlx::Row;

use crate::db::MetadataStore;
use crate::registry::stored;
use crate::routes::AppState;
use crate::verify;

#[derive(Debug, Clone)]
pub struct DefinitionRow {
	pub id: String,
	pub kind: String,
	pub genesis_digest: Vec<u8>,
	pub current_digest: Option<Vec<u8>>,
}

impl MetadataStore {
	pub async fn create_definition(
		&self,
		id: &str,
		kind: &str,
		genesis_digest: &[u8],
		created_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT INTO definitions (id, kind, genesis_digest, created_at) VALUES ($1, $2, $3, $4)")
			.bind(id)
			.bind(kind)
			.bind(genesis_digest)
			.bind(created_at)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn definition(&self, id: &str) -> Result<Option<DefinitionRow>, sqlx::Error> {
		let row = sqlx::query("SELECT id, kind, genesis_digest, current_digest FROM definitions WHERE id = $1")
			.bind(id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.map(|row| DefinitionRow {
			id: row.get("id"),
			kind: row.get("kind"),
			genesis_digest: row.get("genesis_digest"),
			current_digest: row.get("current_digest"),
		}))
	}

	pub async fn definitions_by_kind(&self, kind: &str) -> Result<Vec<DefinitionRow>, sqlx::Error> {
		let rows =
			sqlx::query("SELECT id, kind, genesis_digest, current_digest FROM definitions WHERE kind = $1 ORDER BY id ASC")
				.bind(kind)
				.fetch_all(&self.pool)
				.await?;
		Ok(rows
			.into_iter()
			.map(|row| DefinitionRow {
				id: row.get("id"),
				kind: row.get("kind"),
				genesis_digest: row.get("genesis_digest"),
				current_digest: row.get("current_digest"),
			})
			.collect())
	}

	pub async fn set_definition_current(&self, id: &str, current_digest: &[u8]) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE definitions SET current_digest = $1 WHERE id = $2")
			.bind(current_digest)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}
}

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/games", get(list_games).post(import_game))
		.route("/v1/games/{id}", get(get_game))
		.route("/v1/games/{id}/definitions", post(put_game_definition))
		.route("/v1/loaders", get(list_loaders).post(import_loader))
		.route("/v1/loaders/{id}", get(get_loader))
		.route("/v1/loaders/{id}/definitions", post(put_loader_definition))
		.route("/v1/runtimes", get(list_runtimes).post(import_runtime))
		.route("/v1/runtimes/{id}", get(get_runtime))
		.route("/v1/runtimes/{id}/definitions", post(put_runtime_definition))
		.merge(super::loader_releases::routes())
}

async fn import_game(State(state): State<AppState>, body: Bytes) -> Response {
	import_genesis(&state, GenesisKind::Game, body).await
}

async fn import_loader(State(state): State<AppState>, body: Bytes) -> Response {
	import_genesis(&state, GenesisKind::Loader, body).await
}

async fn import_runtime(State(state): State<AppState>, body: Bytes) -> Response {
	import_genesis(&state, GenesisKind::Runtime, body).await
}

async fn put_game_definition(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
	put_definition(&state, GenesisKind::Game, ObjectKind::GameDef, &id, body).await
}

async fn put_loader_definition(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
	put_definition(&state, GenesisKind::Loader, ObjectKind::LoaderDef, &id, body).await
}

async fn put_runtime_definition(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
	put_definition(&state, GenesisKind::Runtime, ObjectKind::RuntimeDef, &id, body).await
}

async fn list_games(State(state): State<AppState>) -> Response {
	list_definitions(&state, GenesisKind::Game).await
}

async fn list_loaders(State(state): State<AppState>) -> Response {
	list_definitions(&state, GenesisKind::Loader).await
}

async fn list_runtimes(State(state): State<AppState>) -> Response {
	list_definitions(&state, GenesisKind::Runtime).await
}

#[derive(Serialize)]
struct DefinitionSummary {
	id: String,
	kind: &'static str,
	current: Option<String>,
	display_name: Option<String>,
}

async fn list_definitions(state: &AppState, expected: GenesisKind) -> Response {
	let rows = match state.metadata.definitions_by_kind(expected.as_str()).await {
		Ok(rows) => rows,
		Err(error) => return storage_error(error),
	};
	let mut summaries = Vec::with_capacity(rows.len());
	for row in rows {
		let display_name = match row.current_digest.as_deref() {
			Some(digest) => definition_display_name(state, expected, digest).await,
			None => None,
		};
		summaries.push(DefinitionSummary {
			id: row.id,
			kind: expected.as_str(),
			current: row.current_digest.as_deref().map(id_for),
			display_name,
		});
	}
	Json(summaries).into_response()
}

async fn definition_display_name(state: &AppState, expected: GenesisKind, digest: &[u8]) -> Option<String> {
	let object = state.metadata.object(digest).await.ok().flatten()?;
	Some(match expected {
		GenesisKind::Game => GameDef::from_canonical_bytes(&object.payload).ok()?.display_name,
		GenesisKind::Loader => match LoaderObject::from_canonical_bytes(&object.payload).ok()? {
			LoaderObject::Definition(definition) => definition.display_name,
			LoaderObject::Release(_) | LoaderObject::Acceptance(_) => return None,
		},
		GenesisKind::Runtime => RuntimeDef::from_canonical_bytes(&object.payload).ok()?.display_name,
		GenesisKind::Project => return None,
	})
}

async fn get_game(State(state): State<AppState>, Path(id): Path<String>) -> Response {
	get_definition(&state, GenesisKind::Game, &id).await
}

async fn get_loader(State(state): State<AppState>, Path(id): Path<String>) -> Response {
	get_definition(&state, GenesisKind::Loader, &id).await
}

async fn get_runtime(State(state): State<AppState>, Path(id): Path<String>) -> Response {
	get_definition(&state, GenesisKind::Runtime, &id).await
}

#[derive(Serialize)]
struct ImportReceipt {
	id: String,
	kind: &'static str,
}

async fn import_genesis(state: &AppState, expected: GenesisKind, body: Bytes) -> Response {
	let (_root, object) = match verify::verify_genesis(&body) {
		Ok(verified) => verified,
		Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	};
	let genesis = match Genesis::from_canonical_bytes(&object.payload_bytes) {
		Ok(genesis) => genesis,
		Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	};
	if genesis.kind != expected {
		return (StatusCode::BAD_REQUEST, format!("genesis is not a {}", expected.as_str())).into_response();
	}
	match import_definition(state, expected, &object).await {
		Ok(()) => {
			let id = object.id;
			(
				StatusCode::CREATED,
				Json(ImportReceipt {
					id,
					kind: expected.as_str(),
				}),
			)
				.into_response()
		}
		Err((status, message)) => (status, message).into_response(),
	}
}

async fn import_definition(
	state: &AppState,
	expected: GenesisKind,
	object: &crate::verify::VerifiedObject,
) -> Result<(), (StatusCode, String)> {
	match state.metadata.definition(&object.id).await {
		Ok(Some(existing)) if existing.genesis_digest != object.digest.to_vec() => {
			Err((StatusCode::CONFLICT, "id is already bound to a different genesis".to_string()))
		}
		Ok(Some(_)) => Ok(()),
		Ok(None) => {
			state.metadata.put_object(&stored(object)).await.map_err(store_failure)?;
			state
				.metadata
				.create_definition(&object.id, expected.as_str(), &object.digest, now())
				.await
				.map_err(store_failure)?;
			Ok(())
		}
		Err(error) => Err(store_failure(error)),
	}
}

fn loader_names_identity(payload: &[u8], id: &str) -> Result<(), String> {
	let Ok(loader) = LoaderObject::from_canonical_bytes(payload) else {
		return Ok(());
	};
	let named = match &loader {
		LoaderObject::Definition(definition) => &definition.loader_id,
		LoaderObject::Release(release) => &release.loader_id,
		LoaderObject::Acceptance(acceptance) => &acceptance.accepting_loader_id,
	};
	if named != id {
		return Err("the definition names a different loader than the identity it is stored under".to_string());
	}
	Ok(())
}

fn game_revision_is_forward_only(id: &str, previous: Option<&GameDef>, next: &GameDef) -> Result<(), String> {
	if next.game_id != id {
		return Err("the definition names a different game than the identity it is stored under".to_string());
	}
	let Some(previous) = previous else {
		return Ok(());
	};
	if previous.version_ordering != next.version_ordering {
		return Err("the version ordering scheme is fixed by the identity and cannot change".to_string());
	}
	for version in &previous.version_catalog {
		if !next.version_catalog.contains(version) {
			return Err(format!(
				"version `{version}` was removed; a definition catalogue is append-only"
			));
		}
	}
	for category in &previous.categories {
		if !next.categories.iter().any(|candidate| candidate.id == category.id) {
			return Err(format!(
				"category `{}` was removed; a definition catalogue is append-only",
				category.id
			));
		}
	}
	for tag in &previous.tags {
		if !next.tags.iter().any(|candidate| candidate.id == tag.id) {
			return Err(format!("tag `{}` was removed; a definition catalogue is append-only", tag.id));
		}
	}
	Ok(())
}

fn loader_game_id(payload: &[u8]) -> Option<String> {
	match LoaderObject::from_canonical_bytes(payload).ok()? {
		LoaderObject::Definition(definition) => Some(definition.game_id),
		LoaderObject::Acceptance(acceptance) => Some(acceptance.game_id),
		LoaderObject::Release(_) => None,
	}
}

async fn ensure_game_permits_loader(state: &AppState, loader_id: &str, game_id: &str) -> Result<(), (StatusCode, String)> {
	let game = match load_definition(state, GenesisKind::Game, game_id).await {
		Ok(definition) => definition,
		Err(response) if response.status() == StatusCode::NOT_FOUND => return Ok(()),
		Err(response) if response.status() == StatusCode::BAD_REQUEST => return Ok(()),
		Err(response) => return Err((response.status(), "the game definition could not be read".to_string())),
	};
	let Some(current) = game.current_digest else {
		return Ok(());
	};
	let Some(object) = state.metadata.object(&current).await.map_err(store_failure)? else {
		return Ok(());
	};
	let Ok(definition) = GameDef::from_canonical_bytes(&object.payload) else {
		return Ok(());
	};
	if !definition.loaders_allowed {
		return Err((StatusCode::BAD_REQUEST, format!("game `{game_id}` does not permit loaders")));
	}
	if !definition.loader_authorities.is_empty()
		&& !definition.loader_authorities.iter().any(|authority| authority == loader_id)
	{
		return Err((
			StatusCode::BAD_REQUEST,
			format!("`{loader_id}` is not a loader authority for `{game_id}`"),
		));
	}
	Ok(())
}

fn is_loader_definition(payload: &[u8]) -> bool {
	matches!(LoaderObject::from_canonical_bytes(payload), Ok(LoaderObject::Definition(_)))
}

fn store_failure(error: sqlx::Error) -> (StatusCode, String) {
	tracing::error!(%error, "definition store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error".to_string())
}

pub async fn load_directory(state: &AppState, directory: &std::path::Path) -> Result<usize, String> {
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
	for path in &files {
		let bytes = tokio::fs::read(path).await.map_err(|error| error.to_string())?;
		if verify::verify_genesis(&bytes).is_ok() {
			continue;
		}
		let Some((expected, kind, id)) = definition_target(&bytes) else {
			continue;
		};
		match store_definition_version(state, expected, kind, &id, &bytes).await {
			Ok(_) => loaded += 1,
			Err((_, message)) => return Err(format!("{id}: {message}")),
		}
	}
	Ok(loaded)
}

fn definition_target(bytes: &[u8]) -> Option<(GenesisKind, ObjectKind, String)> {
	if let Ok(signed) = SignedObject::<GameDef>::from_bytes(bytes) {
		return Some((GenesisKind::Game, ObjectKind::GameDef, signed.payload.game_id));
	}
	if let Ok(signed) = SignedObject::<LoaderObject>::from_bytes(bytes) {
		let loader_id = match &signed.payload {
			LoaderObject::Definition(definition) => definition.loader_id.clone(),
			LoaderObject::Release(release) => release.loader_id.clone(),
			LoaderObject::Acceptance(acceptance) => acceptance.accepting_loader_id.clone(),
		};
		return Some((GenesisKind::Loader, ObjectKind::LoaderDef, loader_id));
	}
	if let Ok(signed) = SignedObject::<RuntimeDef>::from_bytes(bytes) {
		return Some((GenesisKind::Runtime, ObjectKind::RuntimeDef, signed.payload.runtime_id));
	}
	None
}

#[derive(Serialize)]
struct DefinitionView {
	id: String,
	kind: &'static str,
	genesis: String,
	current: String,
	payload: serde_json::Value,
}

async fn put_definition(state: &AppState, expected: GenesisKind, kind: ObjectKind, id: &str, body: Bytes) -> Response {
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

async fn get_definition(state: &AppState, expected: GenesisKind, id: &str) -> Response {
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

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "definition store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
