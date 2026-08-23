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

	pub async fn index_loader_release(
		&self,
		loader_id: &str,
		version_id: &str,
		object_digest: &[u8],
		declared_time: i64,
	) -> Result<(), sqlx::Error> {
		let existing = sqlx::query("SELECT object_digest FROM loader_releases WHERE loader_id = $1 AND version_id = $2")
			.bind(loader_id)
			.bind(version_id)
			.fetch_optional(&self.pool)
			.await?;
		match existing {
			Some(row) => {
				let recorded: Vec<u8> = row.get("object_digest");
				if recorded != object_digest {
					return Err(sqlx::Error::Protocol(
						"loader release version is already bound to different bytes".to_string(),
					));
				}
				Ok(())
			}
			None => {
				sqlx::query(
					"INSERT INTO loader_releases (loader_id, version_id, object_digest, declared_time) VALUES ($1, $2, $3, $4)",
				)
				.bind(loader_id)
				.bind(version_id)
				.bind(object_digest)
				.bind(declared_time)
				.execute(&self.pool)
				.await?;
				Ok(())
			}
		}
	}

	pub async fn loader_releases_for(&self, loader_id: &str) -> Result<Vec<Vec<u8>>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT object_digest FROM loader_releases WHERE loader_id = $1 ORDER BY declared_time DESC, version_id ASC",
		)
		.bind(loader_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(|row| row.get("object_digest")).collect())
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
		.route("/v1/loaders/{id}/releases", get(list_loader_releases))
		.route("/v1/loaders/{id}/definitions", post(put_loader_definition))
		.route("/v1/runtimes", get(list_runtimes).post(import_runtime))
		.route("/v1/runtimes/{id}", get(get_runtime))
		.route("/v1/runtimes/{id}/definitions", post(put_runtime_definition))
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

#[derive(Serialize)]
struct LoaderReleaseView {
	version: String,
	release: String,
	declared_time: i64,
	game_version_predicate: serde_json::Value,
	runtime_id: Option<String>,
	runtime_predicate: Option<serde_json::Value>,
}

async fn list_loader_releases(State(state): State<AppState>, Path(id): Path<String>) -> Response {
	if let Err(response) = load_definition(&state, GenesisKind::Loader, &id).await {
		return *response;
	}
	let rows = match state.metadata.loader_releases_for(&id).await {
		Ok(rows) => rows,
		Err(error) => return storage_error(error),
	};
	let mut releases = Vec::with_capacity(rows.len());
	for digest in rows {
		let Some(object) = (match state.metadata.object(&digest).await {
			Ok(object) => object,
			Err(error) => return storage_error(error),
		}) else {
			continue;
		};
		let Ok(LoaderObject::Release(release)) = LoaderObject::from_canonical_bytes(&object.payload) else {
			continue;
		};
		releases.push(LoaderReleaseView {
			version: release.version_id,
			release: id_for(&digest),
			declared_time: release.declared_time,
			game_version_predicate: crate::registry::views::predicate_json(&release.game_version_predicate),
			runtime_id: release.runtime_id,
			runtime_predicate: release.runtime_predicate.as_ref().map(crate::registry::views::predicate_json),
		});
	}
	Json(releases).into_response()
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
	state.metadata.put_object(&stored(&object)).await.map_err(store_failure)?;
	if expected == GenesisKind::Loader
		&& let Ok(LoaderObject::Release(release)) = LoaderObject::from_canonical_bytes(&object.payload_bytes)
		&& let Err(error) = state
			.metadata
			.index_loader_release(&release.loader_id, &release.version_id, &object.digest, release.declared_time)
			.await
	{
		if let sqlx::Error::Protocol(message) = &error {
			return Err((StatusCode::CONFLICT, message.clone()));
		}
		return Err(store_failure(error));
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

async fn load_definition(state: &AppState, expected: GenesisKind, id: &str) -> Result<DefinitionRow, Box<Response>> {
	match state.metadata.definition(id).await {
		Ok(Some(definition)) if definition.kind == expected.as_str() => Ok(definition),
		Ok(Some(_)) => Err(Box::new(
			(StatusCode::BAD_REQUEST, "id is not that kind of definition").into_response(),
		)),
		Ok(None) => Err(Box::new((StatusCode::NOT_FOUND, "no such definition").into_response())),
		Err(error) => Err(Box::new(storage_error(error))),
	}
}

fn id_for(digest: &[u8]) -> String {
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

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use axum::body::{Body, to_bytes};
	use moraine_crypto::SigningKey;
	use moraine_model::definition::{GameDef, LoaderDef, LoaderRelease, VersionSyntax};
	use moraine_model::genesis::RootKey;
	use moraine_model::signed::sign_payload;
	use tower::ServiceExt;

	use super::*;
	use crate::blob::BlobStore;
	use crate::capability::Capability;

	async fn state(directory: &std::path::Path) -> AppState {
		let store = Arc::new(BlobStore::new(directory).await.expect("blob store"));
		let metadata = Arc::new(
			MetadataStore::open(directory.join("metadata.sqlite"))
				.await
				.expect("metadata"),
		);
		let config = crate::config::Config {
			bind: "127.0.0.1:0".parse().expect("addr"),
			data_dir: directory.to_path_buf(),
			max_artifact_bytes: 1024,
			max_upload_bytes_per_account: 5_368_709_120,
			max_feed_page_entries: 100,
			max_response_bytes: 16_777_216,
			staging_retention_seconds: 3_600,
			blob_retention_seconds: 604_800,
			max_sync_pages: 200,
			requests_per_minute: 600,
			max_concurrent_syncs: 4,
			maintenance_interval_seconds: 3_600,
			tls_extra_roots: None,
			max_feed_scan_pages: 50,
			skip_migrate_on_start: false,
			database_url: None,
			allow_insecure_federation_local: false,
			publishing: crate::config::Publishing::Open,
			web_dir: None,
		};

		AppState {
			store,
			metadata,
			capability: Arc::new(Capability::discover(&config)),
			login_limiter: std::sync::Arc::new(crate::auth::LoginLimiter::new()),
			metrics: std::sync::Arc::new(crate::ops::metrics::Metrics::new()),
			rate_limiter: std::sync::Arc::new(crate::auth::ratelimit::RateLimiter::new()),
			web_dir: None,
		}
	}

	async fn app() -> (Router, tempfile::TempDir) {
		let directory = tempfile::tempdir().expect("tempdir");
		let state = state(directory.path()).await;
		(crate::routes::router(state), directory)
	}

	async fn body_json(response: Response) -> serde_json::Value {
		let bytes = to_bytes(response.into_body(), 64 * 1024).await.expect("body");
		serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
	}

	fn game_genesis(key: &SigningKey) -> Vec<u8> {
		let genesis = Genesis {
			protocol: 1,
			kind: GenesisKind::Game,
			nonce: vec![0x21; 16],
			roots: vec![RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).expect("root")],
			threshold: 1,
			authorized_kinds: vec!["delegation".to_string(), "game-def".to_string()],
			home_hint: None,
			contacts: None,
			created_at: 1_760_000_000,
		};
		sign_payload(ObjectKind::Genesis, &genesis, &[key]).wire_bytes()
	}

	async fn publish_loader_definition(application: &Router, loader_id: &str, body: Vec<u8>) -> String {
		let put = axum::http::Request::post(format!("/v1/loaders/{loader_id}/definitions"))
			.body(Body::from(body))
			.expect("request");
		let response = application.clone().oneshot(put).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);
		body_json(response).await["definition"]
			.as_str()
			.expect("definition id")
			.to_string()
	}

	fn loader_genesis(key: &SigningKey) -> Vec<u8> {
		let genesis = Genesis {
			protocol: 1,
			kind: GenesisKind::Loader,
			nonce: vec![0x31; 16],
			roots: vec![RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).expect("root")],
			threshold: 1,
			authorized_kinds: vec!["delegation".to_string(), "loader-def".to_string()],
			home_hint: None,
			contacts: None,
			created_at: 1_760_000_000,
		};
		sign_payload(ObjectKind::Genesis, &genesis, &[key]).wire_bytes()
	}

	fn loader_definition(key: &SigningKey, loader_id: &str) -> Vec<u8> {
		sign_payload(
			ObjectKind::LoaderDef,
			&LoaderObject::Definition(LoaderDef {
				protocol: 1,
				loader_id: loader_id.to_string(),
				game_id: "gd:sha256:00".to_string(),
				display_name: "Fabric".to_string(),
				version_ordering: "semver".to_string(),
				bootstrap: None,
				accepted_artifacts: None,
				declared_time: 1_760_000_000,
			}),
			&[key],
		)
		.wire_bytes()
	}

	fn game_definition(key: &SigningKey, game_id: &str) -> Vec<u8> {
		let definition = GameDef {
			protocol: 1,
			game_id: game_id.to_string(),
			display_name: "Minecraft".to_string(),
			version_syntax: VersionSyntax {
				kind: "semver".to_string(),
				pattern: None,
			},
			version_ordering: "semver".to_string(),
			loaders_allowed: true,
			loader_authorities: Vec::new(),
			categories: Vec::new(),
			tags: Vec::new(),
			metadata_extractor: None,
			install_adapter: None,
			declared_time: 1_760_000_000,
		};
		sign_payload(ObjectKind::GameDef, &definition, &[key]).wire_bytes()
	}

	#[tokio::test]
	async fn hosts_a_game_definition() {
		let (application, _directory) = app().await;
		let key = SigningKey::from_seed(&[51u8; 32]);

		let import = axum::http::Request::post("/v1/games")
			.body(Body::from(game_genesis(&key)))
			.expect("request");
		let response = application.clone().oneshot(import).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);
		let game_id = body_json(response).await["id"].as_str().expect("game id").to_string();

		let put = axum::http::Request::post(format!("/v1/games/{game_id}/definitions"))
			.body(Body::from(game_definition(&key, &game_id)))
			.expect("request");
		let response = application.clone().oneshot(put).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let get = axum::http::Request::get(format!("/v1/games/{game_id}"))
			.body(Body::empty())
			.expect("request");
		let response = application.clone().oneshot(get).await.expect("response");
		assert_eq!(response.status(), StatusCode::OK);
		let view = body_json(response).await;
		assert_eq!(view["kind"], "game");
		assert_eq!(view["payload"]["display_name"], "Minecraft");
		assert_eq!(view["payload"]["version_ordering"], "semver");

		let list = axum::http::Request::get("/v1/games").body(Body::empty()).expect("request");
		let response = application.clone().oneshot(list).await.expect("response");
		assert_eq!(response.status(), StatusCode::OK);
		let games = body_json(response).await;
		assert_eq!(games.as_array().expect("games").len(), 1);
		assert_eq!(games[0]["id"], game_id);
		assert_eq!(games[0]["kind"], "game");
		assert!(games[0]["current"].as_str().is_some());
		assert_eq!(games[0]["display_name"], "Minecraft");

		let loaders = axum::http::Request::get("/v1/loaders").body(Body::empty()).expect("request");
		let response = application.clone().oneshot(loaders).await.expect("response");
		let loaders = body_json(response).await;
		assert!(loaders.as_array().expect("loaders").is_empty());

		let wrong_kind = axum::http::Request::post("/v1/loaders")
			.body(Body::from(game_genesis(&key)))
			.expect("request");
		let response = application.oneshot(wrong_kind).await.expect("response");
		assert_eq!(response.status(), StatusCode::BAD_REQUEST);
	}

	#[tokio::test]
	async fn keeps_the_loader_definition_current_when_a_release_arrives() {
		use moraine_model::compatibility::{Predicate, Scheme};

		let key = SigningKey::from_seed(&[0x66; 32]);
		let (application, directory) = app().await;
		let genesis = axum::http::Request::post("/v1/loaders")
			.body(Body::from(loader_genesis(&key)))
			.expect("request");
		let response = application.clone().oneshot(genesis).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);
		let loader_id = body_json(response).await["id"].as_str().expect("id").to_string();

		let definition_id = publish_loader_definition(&application, &loader_id, loader_definition(&key, &loader_id)).await;

		let release = sign_payload(
			ObjectKind::LoaderDef,
			&LoaderObject::Release(LoaderRelease {
				protocol: 1,
				loader_id: loader_id.clone(),
				version_id: "0.15.0".to_string(),
				game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
				runtime_id: None,
				runtime_predicate: None,
				bootstrap: None,
				declared_time: 1_760_000_000,
			}),
			&[&key],
		);
		publish_loader_definition(&application, &loader_id, release.wire_bytes()).await;

		let list = axum::http::Request::get("/v1/loaders").body(Body::empty()).expect("request");
		let response = application.clone().oneshot(list).await.expect("response");
		let loaders = body_json(response).await;
		assert_eq!(loaders[0]["display_name"], "Fabric");
		assert_eq!(loaders[0]["current"], definition_id);

		let definitions = directory.path().join("definitions");
		std::fs::create_dir(&definitions).expect("create");
		std::fs::write(definitions.join("loader.genesis"), loader_genesis(&key)).expect("write");
		std::fs::write(definitions.join("loader.release"), release.wire_bytes()).expect("write");
		let state = state(directory.path()).await;
		let loaded = load_directory(&state, &definitions).await.expect("load");
		assert_eq!(loaded, 2);
	}

	#[tokio::test]
	async fn lists_loader_releases_and_refuses_to_rewrite_a_version() {
		use moraine_model::compatibility::{Predicate, Scheme};

		let key = SigningKey::from_seed(&[0x67; 32]);
		let (application, _directory) = app().await;
		let genesis = axum::http::Request::post("/v1/loaders")
			.body(Body::from(loader_genesis(&key)))
			.expect("request");
		let response = application.clone().oneshot(genesis).await.expect("response");
		let loader_id = body_json(response).await["id"].as_str().expect("id").to_string();
		publish_loader_definition(&application, &loader_id, loader_definition(&key, &loader_id)).await;

		let release = |version: &str| {
			sign_payload(
				ObjectKind::LoaderDef,
				&LoaderObject::Release(LoaderRelease {
					protocol: 1,
					loader_id: loader_id.clone(),
					version_id: version.to_string(),
					game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
					runtime_id: Some("gd:sha256:cd".to_string()),
					runtime_predicate: Some(Predicate::new(Scheme::Exact, vec!["17".to_string()])),
					bootstrap: None,
					declared_time: 1_760_000_000,
				}),
				&[&key],
			)
		};
		let put = axum::http::Request::post(format!("/v1/loaders/{loader_id}/definitions"))
			.body(Body::from(release("0.15.0").wire_bytes()))
			.expect("request");
		let response = application.clone().oneshot(put).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let list = axum::http::Request::get(format!("/v1/loaders/{loader_id}/releases"))
			.body(Body::empty())
			.expect("request");
		let response = application.clone().oneshot(list).await.expect("response");
		assert_eq!(response.status(), StatusCode::OK);
		let releases = body_json(response).await;
		assert_eq!(releases.as_array().expect("releases").len(), 1);
		assert_eq!(releases[0]["version"], "0.15.0");
		assert_eq!(releases[0]["runtime_id"], "gd:sha256:cd");
		assert_eq!(releases[0]["runtime_predicate"]["values"][0], "17");

		let rewritten = sign_payload(
			ObjectKind::LoaderDef,
			&LoaderObject::Release(LoaderRelease {
				protocol: 1,
				loader_id: loader_id.clone(),
				version_id: "0.15.0".to_string(),
				game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.19.0".to_string()]),
				runtime_id: None,
				runtime_predicate: None,
				bootstrap: None,
				declared_time: 1_760_000_100,
			}),
			&[&key],
		);
		let put = axum::http::Request::post(format!("/v1/loaders/{loader_id}/definitions"))
			.body(Body::from(rewritten.wire_bytes()))
			.expect("request");
		let response = application.oneshot(put).await.expect("response");
		assert_eq!(response.status(), StatusCode::CONFLICT);
	}

	#[tokio::test]
	async fn loads_a_definition_file_from_the_data_directory() {
		let directory = tempfile::tempdir().expect("tempdir");
		let definitions = directory.path().join("definitions");
		std::fs::create_dir(&definitions).expect("create");
		let signer = SigningKey::from_seed(&[0x77; 32]);
		let wire = game_genesis(&signer);
		let (_, object) = crate::verify::verify_genesis(&wire).expect("verify");
		let game_id = object.id;
		std::fs::write(definitions.join("minecraft"), &wire).expect("write");
		std::fs::write(definitions.join("minecraft.json"), game_definition(&signer, &game_id)).expect("write");
		std::fs::write(definitions.join("not-an-object"), b"junk").expect("write");
		let state = state(directory.path()).await;

		let loaded = load_directory(&state, &definitions).await.expect("load");

		assert_eq!(loaded, 2);
		let request = axum::http::Request::get(format!("/v1/games/{game_id}"))
			.body(Body::empty())
			.expect("request");
		let response = crate::routes::router(state).oneshot(request).await.expect("response");
		let status = response.status();
		let body = to_bytes(response.into_body(), 64 * 1024).await.expect("body");
		assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
		let view: serde_json::Value = serde_json::from_slice(&body).expect("json");
		assert_eq!(view["payload"]["display_name"], "Minecraft");
	}
}
