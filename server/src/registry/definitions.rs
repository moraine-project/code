use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_crypto::ObjectKind;
use moraine_model::Canonical;
use moraine_model::definition::{GameDef, LoaderObject, RuntimeDef};
use moraine_model::genesis::{Genesis, GenesisKind};
use serde::{Deserialize, Serialize};
use sqlx::Row;

pub(super) use super::catalog::{get_definition, put_definition};
pub(crate) use super::catalog::{id_for, load_definition, load_directory};
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
	pub source_home: Option<String>,
}

impl MetadataStore {
	pub async fn create_definition(
		&self,
		id: &str,
		kind: &str,
		genesis_digest: &[u8],
		source_home: Option<&str>,
		created_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO definitions (id, kind, genesis_digest, source_home, created_at) VALUES ($1, $2, $3, $4, $5)",
		)
		.bind(id)
		.bind(kind)
		.bind(genesis_digest)
		.bind(source_home)
		.bind(created_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn definition(&self, id: &str) -> Result<Option<DefinitionRow>, sqlx::Error> {
		let row = sqlx::query("SELECT id, kind, genesis_digest, current_digest, source_home FROM definitions WHERE id = $1")
			.bind(id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.map(|row| DefinitionRow {
			id: row.get("id"),
			kind: row.get("kind"),
			genesis_digest: row.get("genesis_digest"),
			current_digest: row.get("current_digest"),
			source_home: row.get("source_home"),
		}))
	}

	pub async fn definitions_by_kind(&self, kind: &str) -> Result<Vec<DefinitionRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, kind, genesis_digest, current_digest, source_home FROM definitions WHERE kind = $1 ORDER BY id ASC",
		)
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
				source_home: row.get("source_home"),
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

async fn list_loaders(State(state): State<AppState>, Query(query): Query<LoaderQuery>) -> Response {
	list_loader_definitions(&state, &query).await
}

#[derive(Deserialize, Default)]
struct LoaderQuery {
	#[serde(default)]
	game: Option<String>,
	#[serde(default)]
	game_version: Option<String>,
}

async fn list_loader_definitions(state: &AppState, query: &LoaderQuery) -> Response {
	let rows = match state.metadata.definitions_by_kind(GenesisKind::Loader.as_str()).await {
		Ok(rows) => rows,
		Err(error) => return storage_error(error),
	};
	let filtering = query.game.is_some() || query.game_version.is_some();
	let mut summaries = Vec::with_capacity(rows.len());
	for row in rows {
		let Some(current) = row.current_digest.as_deref() else {
			if !filtering {
				summaries.push(DefinitionSummary {
					id: row.id,
					kind: GenesisKind::Loader.as_str(),
					current: None,
					display_name: None,
					source_home: row.source_home,
				});
			}
			continue;
		};
		let Some(object) = (match state.metadata.object(current).await {
			Ok(object) => object,
			Err(error) => return storage_error(error),
		}) else {
			continue;
		};
		let Ok(LoaderObject::Definition(definition)) = LoaderObject::from_canonical_bytes(&object.payload) else {
			continue;
		};
		if query.game.as_deref().is_some_and(|game| definition.game_id != game) {
			continue;
		}
		if let Some(version) = query.game_version.as_deref() {
			let catalog = crate::registry::compatibility::catalog_for_game(state, &definition.game_id).await;
			let supported = definition.game_versions.as_ref().is_some_and(|predicate| {
				crate::registry::compatibility::predicate_satisfied(predicate, version, catalog.as_ref())
			});
			if !supported {
				continue;
			}
		}
		summaries.push(DefinitionSummary {
			id: row.id,
			kind: GenesisKind::Loader.as_str(),
			current: Some(id_for(current)),
			display_name: Some(definition.display_name),
			source_home: row.source_home,
		});
	}
	Json(summaries).into_response()
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
	source_home: Option<String>,
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
			source_home: row.source_home,
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

pub(crate) async fn import_definition(
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
			if let Some(message) = definition_quota_reached(state).await? {
				return Err((StatusCode::FORBIDDEN, message));
			}
			state.metadata.put_object(&stored(object)).await.map_err(store_failure)?;
			state
				.metadata
				.create_definition(&object.id, expected.as_str(), &object.digest, None, now())
				.await
				.map_err(store_failure)?;
			Ok(())
		}
		Err(error) => Err(store_failure(error)),
	}
}

pub(crate) fn loader_names_identity(payload: &[u8], id: &str) -> Result<(), String> {
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

pub(crate) fn game_revision_is_forward_only(id: &str, previous: Option<&GameDef>, next: &GameDef) -> Result<(), String> {
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

pub(crate) fn loader_game_id(payload: &[u8]) -> Option<String> {
	match LoaderObject::from_canonical_bytes(payload).ok()? {
		LoaderObject::Definition(definition) => Some(definition.game_id),
		LoaderObject::Acceptance(acceptance) => Some(acceptance.game_id),
		LoaderObject::Release(_) => None,
	}
}

pub(crate) async fn ensure_game_permits_loader(
	state: &AppState,
	loader_id: &str,
	game_id: &str,
) -> Result<(), (StatusCode, String)> {
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

pub(crate) fn is_loader_definition(payload: &[u8]) -> bool {
	matches!(LoaderObject::from_canonical_bytes(payload), Ok(LoaderObject::Definition(_)))
}

pub(crate) async fn definition_quota_reached(state: &AppState) -> Result<Option<String>, (StatusCode, String)> {
	if state.capability.max_definitions == 0 {
		return Ok(None);
	}
	let held = state.metadata.table_count("definitions").await.map_err(store_failure)?;
	if held.max(0) as u64 >= state.capability.max_definitions {
		return Ok(Some("this instance has reached its definition limit".to_string()));
	}
	Ok(None)
}

pub(crate) fn store_failure(error: sqlx::Error) -> (StatusCode, String) {
	tracing::error!(%error, "definition store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error".to_string())
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

pub(crate) fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "definition store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
