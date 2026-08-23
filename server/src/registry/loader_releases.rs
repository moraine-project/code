use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::Canonical;
use moraine_model::definition::LoaderObject;
use moraine_model::genesis::GenesisKind;
use serde::Serialize;
use sqlx::Row;

use super::definitions::{id_for, load_definition};
use super::storage_error;
use crate::db::MetadataStore;
use crate::routes::AppState;

pub(crate) fn routes() -> Router<AppState> {
	Router::new().route("/v1/loaders/{id}/releases", get(list_loader_releases))
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

impl MetadataStore {
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
}

#[cfg(test)]
mod tests {
	use axum::body::Body;
	use axum::http::StatusCode;
	use moraine_crypto::{ObjectKind, SigningKey};
	use moraine_model::compatibility::{Predicate, Scheme};
	use moraine_model::definition::{LoaderObject, LoaderRelease};
	use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
	use moraine_model::signed::sign_payload;
	use tower::ServiceExt;

	use crate::test_support::{app, body_json, loader_definition_wire};

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

	async fn publish_loader_definition(application: &axum::Router, loader_id: &str, body: Vec<u8>) -> String {
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

	#[tokio::test]
	async fn lists_loader_releases_and_refuses_to_rewrite_a_version() {
		let key = SigningKey::from_seed(&[0x67; 32]);
		let (application, _directory) = app().await;
		let genesis = axum::http::Request::post("/v1/loaders")
			.body(Body::from(loader_genesis(&key)))
			.expect("request");
		let response = application.clone().oneshot(genesis).await.expect("response");
		let loader_id = body_json(response).await["id"].as_str().expect("id").to_string();
		publish_loader_definition(
			&application,
			&loader_id,
			loader_definition_wire(&key, &loader_id, "gd:sha256:00"),
		)
		.await;

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
}
