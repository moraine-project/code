use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::StatusCode;
use axum::response::Response;
use moraine_crypto::SigningKey;
use moraine_model::compatibility::Predicate;
use moraine_model::definition::{GameDef, LoaderDef, LoaderObject, LoaderRelease, VersionSyntax};
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use super::definitions::load_directory;
use super::{ObjectKind, Router};
use crate::blob::BlobStore;
use crate::capability::Capability;
use crate::db::MetadataStore;
use crate::routes::AppState;

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
		s3: Default::default(),
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
	loader_definition_for(key, loader_id, "gd:sha256:00")
}

fn loader_definition_for(key: &SigningKey, loader_id: &str, game_id: &str) -> Vec<u8> {
	sign_payload(
		ObjectKind::LoaderDef,
		&LoaderObject::Definition(LoaderDef {
			protocol: 1,
			loader_id: loader_id.to_string(),
			game_id: game_id.to_string(),
			display_name: "Fabric".to_string(),
			version_ordering: "semver".to_string(),
			version_catalog: Vec::new(),
			game_versions: None,
			bootstrap: None,
			accepted_artifacts: None,
			declared_time: 1_760_000_000,
		}),
		&[key],
	)
	.wire_bytes()
}

fn loader_definition_with_game_versions(
	key: &SigningKey,
	loader_id: &str,
	game_id: &str,
	scheme: &str,
	versions: &[&str],
) -> Vec<u8> {
	sign_payload(
		ObjectKind::LoaderDef,
		&LoaderObject::Definition(LoaderDef {
			protocol: 1,
			loader_id: loader_id.to_string(),
			game_id: game_id.to_string(),
			display_name: "Fabric".to_string(),
			version_ordering: "semver".to_string(),
			version_catalog: Vec::new(),
			game_versions: Some(Predicate {
				scheme: scheme.to_string(),
				values: versions.iter().map(|version| version.to_string()).collect(),
			}),
			bootstrap: None,
			accepted_artifacts: None,
			declared_time: 1_760_000_000,
		}),
		&[key],
	)
	.wire_bytes()
}

fn game_definition(key: &SigningKey, game_id: &str) -> Vec<u8> {
	game_definition_with(key, game_id, true, Vec::new())
}

fn game_definition_with(key: &SigningKey, game_id: &str, loaders_allowed: bool, loader_authorities: Vec<String>) -> Vec<u8> {
	game_definition_catalog(key, game_id, "semver", Vec::new(), loaders_allowed, loader_authorities)
}

fn game_definition_catalog(
	key: &SigningKey,
	game_id: &str,
	ordering: &str,
	versions: Vec<&str>,
	loaders_allowed: bool,
	loader_authorities: Vec<String>,
) -> Vec<u8> {
	let definition = GameDef {
		protocol: 1,
		game_id: game_id.to_string(),
		display_name: "Minecraft".to_string(),
		version_syntax: VersionSyntax {
			kind: ordering.to_string(),
			pattern: None,
		},
		version_ordering: ordering.to_string(),
		version_catalog: versions.into_iter().map(str::to_string).collect(),
		loaders_allowed,
		loader_authorities,
		categories: Vec::new(),
		tags: Vec::new(),
		metadata_extractor: None,
		install_adapter: None,
		declared_time: 1_760_000_000,
	};
	sign_payload(ObjectKind::GameDef, &definition, &[key]).wire_bytes()
}

#[tokio::test]
async fn refuses_a_loader_the_game_does_not_permit() {
	let (application, _directory) = app().await;
	let loader_key = SigningKey::from_seed(&[0x81; 32]);
	let game_key = SigningKey::from_seed(&[0x82; 32]);

	let loader_genesis = axum::http::Request::post("/v1/loaders")
		.body(Body::from(loader_genesis(&loader_key)))
		.expect("request");
	let response = application.clone().oneshot(loader_genesis).await.expect("response");
	let loader_id = body_json(response).await["id"].as_str().expect("loader id").to_string();

	let game_genesis = axum::http::Request::post("/v1/games")
		.body(Body::from(game_genesis(&game_key)))
		.expect("request");
	let response = application.clone().oneshot(game_genesis).await.expect("response");
	let game_id = body_json(response).await["id"].as_str().expect("game id").to_string();

	let closed = axum::http::Request::post(format!("/v1/games/{game_id}/definitions"))
		.body(Body::from(game_definition_with(&game_key, &game_id, false, Vec::new())))
		.expect("request");
	let response = application.clone().oneshot(closed).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let put = axum::http::Request::post(format!("/v1/loaders/{loader_id}/definitions"))
		.body(Body::from(loader_definition_for(&loader_key, &loader_id, &game_id)))
		.expect("request");
	let response = application.clone().oneshot(put).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);

	let curated = axum::http::Request::post(format!("/v1/games/{game_id}/definitions"))
		.body(Body::from(game_definition_with(
			&game_key,
			&game_id,
			true,
			vec!["gd:sha256:99".to_string()],
		)))
		.expect("request");
	let response = application.clone().oneshot(curated).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let put = axum::http::Request::post(format!("/v1/loaders/{loader_id}/definitions"))
		.body(Body::from(loader_definition_for(&loader_key, &loader_id, &game_id)))
		.expect("request");
	let response = application.clone().oneshot(put).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);

	let permitted = axum::http::Request::post(format!("/v1/games/{game_id}/definitions"))
		.body(Body::from(game_definition_with(
			&game_key,
			&game_id,
			true,
			vec![loader_id.clone()],
		)))
		.expect("request");
	let response = application.clone().oneshot(permitted).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let put = axum::http::Request::post(format!("/v1/loaders/{loader_id}/definitions"))
		.body(Body::from(loader_definition_for(&loader_key, &loader_id, &game_id)))
		.expect("request");
	let response = application.oneshot(put).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
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

#[tokio::test]
async fn keeps_a_game_definition_catalogue_append_only() {
	let (application, _directory) = app().await;
	let key = SigningKey::from_seed(&[0x83; 32]);

	let genesis = axum::http::Request::post("/v1/games")
		.body(Body::from(game_genesis(&key)))
		.expect("request");
	let response = application.clone().oneshot(genesis).await.expect("response");
	let game_id = body_json(response).await["id"].as_str().expect("game id").to_string();

	let put = |application: Router, ordering: &str, versions: Vec<&str>, game: &str| {
		let body = game_definition_catalog(&key, game, ordering, versions, true, Vec::new());
		let uri = format!("/v1/games/{game_id}/definitions");
		async move {
			let request = axum::http::Request::post(uri).body(Body::from(body)).expect("request");
			application.oneshot(request).await.expect("response").status()
		}
	};

	assert_eq!(
		put(application.clone(), "ordered-list", vec!["1.20", "1.20.1"], &game_id).await,
		StatusCode::CREATED
	);
	assert_eq!(
		put(application.clone(), "ordered-list", vec!["1.20", "1.20.1", "1.21"], &game_id).await,
		StatusCode::CREATED,
		"adding a version is a normal revision"
	);
	assert_eq!(
		put(application.clone(), "ordered-list", vec!["1.20.1", "1.21"], &game_id).await,
		StatusCode::BAD_REQUEST,
		"dropping a version is a rollback"
	);
	assert_eq!(
		put(application.clone(), "semver", vec!["1.20", "1.20.1", "1.21"], &game_id).await,
		StatusCode::BAD_REQUEST,
		"the ordering scheme is fixed by the identity"
	);
	assert_eq!(
		put(application.clone(), "ordered-list", vec!["1.20", "1.20.1", "1.21"], "other").await,
		StatusCode::BAD_REQUEST,
		"the definition cannot name a different game than it is stored under"
	);
}

#[tokio::test]
async fn filters_loaders_by_the_game_version_they_support() {
	let (application, _directory) = app().await;
	let game_key = SigningKey::from_seed(&[0x84; 32]);
	let loader_key = SigningKey::from_seed(&[0x85; 32]);

	let genesis = axum::http::Request::post("/v1/games")
		.body(Body::from(game_genesis(&game_key)))
		.expect("request");
	let response = application.clone().oneshot(genesis).await.expect("response");
	let game_id = body_json(response).await["id"].as_str().expect("game id").to_string();
	let put = axum::http::Request::post(format!("/v1/games/{game_id}/definitions"))
		.body(Body::from(game_definition_catalog(
			&game_key,
			&game_id,
			"ordered-list",
			vec!["1.20", "1.20.1", "1.21"],
			true,
			Vec::new(),
		)))
		.expect("request");
	let response = application.clone().oneshot(put).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let genesis = axum::http::Request::post("/v1/loaders")
		.body(Body::from(loader_genesis(&loader_key)))
		.expect("request");
	let response = application.clone().oneshot(genesis).await.expect("response");
	let loader_id = body_json(response).await["id"].as_str().expect("loader id").to_string();
	let put = axum::http::Request::post(format!("/v1/loaders/{loader_id}/definitions"))
		.body(Body::from(loader_definition_with_game_versions(
			&loader_key,
			&loader_id,
			&game_id,
			"ordered-list",
			&["1.20..=1.20.1"],
		)))
		.expect("request");
	let response = application.clone().oneshot(put).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let list = |application: axum::Router, query: &str| {
		let uri = format!("/v1/loaders?{query}");
		async move {
			let request = axum::http::Request::get(uri).body(Body::empty()).expect("request");
			body_json(application.oneshot(request).await.expect("response")).await
		}
	};

	let all = list(application.clone(), "").await;
	assert_eq!(all.as_array().expect("loaders").len(), 1);
	let supported = list(application.clone(), "game_version=1.20.1").await;
	assert_eq!(supported.as_array().expect("loaders").len(), 1);
	let unsupported = list(application.clone(), "game_version=1.21").await;
	assert!(unsupported.as_array().expect("loaders").is_empty());
	let wrong_game = list(application.clone(), "game=gd:sha256:other").await;
	assert!(wrong_game.as_array().expect("loaders").is_empty());
}
