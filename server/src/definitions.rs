use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_crypto::ObjectKind;
use moraine_model::Canonical;
use moraine_model::genesis::{Genesis, GenesisKind};
use serde::Serialize;
use sqlx::Row;

use crate::registry::stored;
use crate::routes::AppState;
use crate::store::MetadataStore;
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
		sqlx::query("INSERT INTO definitions (id, kind, genesis_digest, created_at) VALUES (?1, ?2, ?3, ?4)")
			.bind(id)
			.bind(kind)
			.bind(genesis_digest)
			.bind(created_at)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn definition(&self, id: &str) -> Result<Option<DefinitionRow>, sqlx::Error> {
		let row = sqlx::query("SELECT id, kind, genesis_digest, current_digest FROM definitions WHERE id = ?1")
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

	pub async fn set_definition_current(&self, id: &str, current_digest: &[u8]) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE definitions SET current_digest = ?1 WHERE id = ?2")
			.bind(current_digest)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}
}

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/games", post(import_game))
		.route("/v1/games/{id}", get(get_game))
		.route("/v1/games/{id}/definitions", post(put_game_definition))
		.route("/v1/loaders", post(import_loader))
		.route("/v1/loaders/{id}", get(get_loader))
		.route("/v1/loaders/{id}/definitions", post(put_loader_definition))
		.route("/v1/runtimes", post(import_runtime))
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
	match state.metadata.definition(&object.id).await {
		Ok(Some(existing)) if existing.genesis_digest != object.digest.to_vec() => {
			return (StatusCode::CONFLICT, "id is already bound to a different genesis").into_response();
		}
		Ok(Some(_)) => {}
		Ok(None) => {
			if let Err(error) = state.metadata.put_object(&stored(&object)).await {
				return storage_error(error);
			}
			if let Err(error) = state
				.metadata
				.create_definition(&object.id, expected.as_str(), &object.digest, now())
				.await
			{
				return storage_error(error);
			}
		}
		Err(error) => return storage_error(error),
	}
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

#[derive(Serialize)]
struct DefinitionView {
	id: String,
	kind: &'static str,
	genesis: String,
	current: String,
	payload: serde_json::Value,
}

async fn put_definition(state: &AppState, expected: GenesisKind, kind: ObjectKind, id: &str, body: Bytes) -> Response {
	let definition = match load_definition(state, expected, id).await {
		Ok(definition) => definition,
		Err(response) => return *response,
	};
	let genesis = match state.metadata.object(&definition.genesis_digest).await {
		Ok(Some(genesis)) => genesis,
		Ok(None) => return (StatusCode::INTERNAL_SERVER_ERROR, "definition genesis is missing").into_response(),
		Err(error) => return storage_error(error),
	};
	let (root, _) = match verify::verify_genesis(&genesis.wire) {
		Ok(verified) => verified,
		Err(error) => return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
	};
	let object = match verify::verify_object(kind, &body, &root) {
		Ok(object) => object,
		Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	};
	if let Err(error) = state.metadata.put_object(&stored(&object)).await {
		return storage_error(error);
	}
	if let Err(error) = state.metadata.set_definition_current(id, &object.digest).await {
		return storage_error(error);
	}
	(StatusCode::CREATED, Json(serde_json::json!({ "definition": object.id }))).into_response()
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
	let payload = match crate::views::payload_json(&object.payload) {
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
	use moraine_model::definition::{GameDef, VersionSyntax};
	use moraine_model::genesis::RootKey;
	use moraine_model::signed::sign_payload;
	use tower::ServiceExt;

	use super::*;
	use crate::blob::BlobStore;
	use crate::capability::Capability;

	async fn app() -> (Router, tempfile::TempDir) {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = Arc::new(BlobStore::new(directory.path()).await.expect("blob store"));
		let metadata = Arc::new(
			MetadataStore::open(directory.path().join("metadata.sqlite"))
				.await
				.expect("metadata"),
		);
		let config = crate::config::Config {
			bind: "127.0.0.1:0".parse().expect("addr"),
			data_dir: directory.path().to_path_buf(),
			max_artifact_bytes: 1024,
			max_feed_page_entries: 100,
			allow_insecure_federation_local: false,
			publishing: crate::config::Publishing::Open,
			web_dir: None,
		};
		let state = AppState {
			store,
			metadata,
			capability: Arc::new(Capability::discover(&config)),
			web_dir: None,
		};
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

		let wrong_kind = axum::http::Request::post("/v1/loaders")
			.body(Body::from(game_genesis(&key)))
			.expect("request");
		let response = application.oneshot(wrong_kind).await.expect("response");
		assert_eq!(response.status(), StatusCode::BAD_REQUEST);
	}
}
