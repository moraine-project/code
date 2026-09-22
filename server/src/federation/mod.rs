pub mod egress;
mod error;
pub mod home;
pub mod mirrors;
pub mod notifications;
mod project_sync;
pub mod subscriptions;
pub mod webhooks;
pub mod witness;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
pub(crate) use error::FederationError;
pub(crate) use home::HomeClient;
pub use mirrors::probe_mirrors;
use moraine_crypto::ObjectKind;
use moraine_model::Canonical;
use moraine_model::genesis::{Genesis, GenesisKind};
use serde::{Deserialize, Serialize};

use self::error::{hex_of, now, rejected, storage, storage_error};
use self::project_sync::sync_loader_releases;
use crate::auth::AuthenticatedUser;
use crate::registry::{self, load_delegations};
use crate::routes::AppState;
use crate::verify;

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/federation/sync", post(project_sync::sync_handler))
		.route("/v1/federation/resync", post(resync_handler))
		.route("/v1/federation/sync-definition", post(sync_definition_handler))
		.route("/v1/federation/subscribe-definition", post(subscribe_definition_handler))
		.route("/v1/definition-subscriptions", get(list_definition_subscriptions))
		.route(
			"/v1/subscriptions",
			get(project_sync::list_subscriptions).delete(project_sync::unsubscribe),
		)
		.route("/v1/subscriptions/reset", post(project_sync::reset_subscription))
		.route("/v1/federation/witness", post(witness::import))
		.route("/v1/federation/witness/{project_id}", get(witness::export))
		.route("/v1/projects/{id}/witness", get(witness::observe))
}

#[derive(Debug, Clone, Deserialize)]
struct SubscribeDefinitionRequest {
	home_url: String,
	id: String,
	kind: String,
}

async fn subscribe_definition_handler(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<SubscribeDefinitionRequest>,
) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let home_url = request.home_url.trim_end_matches('/').to_string();
	match sync_definition(&state, &home_url, &request.id, &request.kind).await {
		Ok(report) => {
			if let Err(error) = state
				.metadata
				.upsert_definition_subscription(&home_url, &request.id, &request.kind, now())
				.await
			{
				return storage_error(error);
			}
			Json(report).into_response()
		}
		Err(error) => {
			let status = match error {
				FederationError::InvalidUrl(_) => StatusCode::BAD_REQUEST,
				FederationError::Rejected(_) => StatusCode::CONFLICT,
				_ => StatusCode::BAD_GATEWAY,
			};
			(status, error.to_string()).into_response()
		}
	}
}

async fn list_definition_subscriptions(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	match state.metadata.definition_subscriptions().await {
		Ok(rows) => Json(
			rows.into_iter()
				.map(|row| {
					serde_json::json!({
						"home_url": row.home_url,
						"id": row.id,
						"kind": row.kind,
						"updated_at": row.updated_at,
					})
				})
				.collect::<Vec<_>>(),
		)
		.into_response(),
		Err(error) => storage_error(error),
	}
}

pub(crate) async fn resync_definitions(state: &AppState) -> Result<usize, FederationError> {
	let limit = state.capability.max_concurrent_syncs.max(1) as usize;
	let subscriptions = state.metadata.definition_subscriptions().await.map_err(storage)?;
	let mut running = tokio::task::JoinSet::new();
	let mut synced = 0;
	let mut failed = 0;
	for subscription in subscriptions {
		if running.len() >= limit {
			account(&mut running, &mut synced, &mut failed).await;
		}
		let state = state.clone();
		running.spawn(async move {
			match sync_definition(&state, &subscription.home_url, &subscription.id, &subscription.kind).await {
				Ok(_) => true,
				Err(error) => {
					state.metrics.record_federation_failure(&error);
					tracing::warn!(%error, id = %subscription.id, "definition resync failed");
					false
				}
			}
		});
	}
	while !running.is_empty() {
		account(&mut running, &mut synced, &mut failed).await;
	}
	Ok(synced)
}

pub async fn resync_subscriptions(state: &AppState) -> Result<error::ResyncReport, FederationError> {
	let limit = state.capability.max_concurrent_syncs.max(1) as usize;
	let subscriptions = state.metadata.subscriptions().await.map_err(storage)?;
	let mut running = tokio::task::JoinSet::new();
	let mut synced = 0;
	let mut failed = 0;
	for subscription in subscriptions {
		if running.len() >= limit {
			account(&mut running, &mut synced, &mut failed).await;
		}
		let state = state.clone();
		running.spawn(async move {
			match project_sync::sync(&state, &subscription.home_url, &subscription.project_id).await {
				Ok(_) => true,
				Err(error) => {
					state.metrics.record_federation_failure(&error);
					tracing::warn!(
						%error,
						home = %subscription.home_url,
						project = %subscription.project_id,
						"subscription resync failed"
					);
					false
				}
			}
		});
	}
	while !running.is_empty() {
		account(&mut running, &mut synced, &mut failed).await;
	}
	Ok(error::ResyncReport { synced, failed })
}

async fn account(running: &mut tokio::task::JoinSet<bool>, synced: &mut usize, failed: &mut usize) {
	match running.join_next().await {
		Some(Ok(true)) => *synced += 1,
		Some(Ok(false)) => *failed += 1,
		Some(Err(error)) => {
			*failed += 1;
			tracing::warn!(%error, "subscription sync task failed");
		}
		None => {}
	}
}

async fn resync_handler(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	match resync_subscriptions(&state).await {
		Ok(report) => Json(report).into_response(),
		Err(error) => {
			tracing::error!(%error, "subscription resync failed");
			(StatusCode::INTERNAL_SERVER_ERROR, "resync failed").into_response()
		}
	}
}

#[derive(Deserialize)]
struct SyncDefinitionRequest {
	home_url: String,
	id: String,
	kind: String,
}

#[derive(Deserialize)]
struct DefinitionSummary {
	id: String,
	genesis: String,
	current: String,
}

#[derive(Deserialize)]
struct LoaderReleaseSummary {
	release: String,
}

#[derive(Serialize)]
pub(crate) struct DefinitionReport {
	id: String,
	kind: String,
	definition: String,
}

async fn sync_definition_handler(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<SyncDefinitionRequest>,
) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let home_url = request.home_url.trim_end_matches('/').to_string();
	match sync_definition(&state, &home_url, &request.id, &request.kind).await {
		Ok(report) => Json(report).into_response(),
		Err(error) => {
			let status = match error {
				FederationError::InvalidUrl(_) => StatusCode::BAD_REQUEST,
				FederationError::Rejected(_) => StatusCode::CONFLICT,
				_ => StatusCode::BAD_GATEWAY,
			};
			(status, error.to_string()).into_response()
		}
	}
}

pub(crate) async fn sync_definition(
	state: &AppState,
	home_url: &str,
	id: &str,
	kind: &str,
) -> Result<DefinitionReport, FederationError> {
	let (genesis_kind, definition_kind) = match kind {
		"game" => (GenesisKind::Game, ObjectKind::GameDef),
		"loader" => (GenesisKind::Loader, ObjectKind::LoaderDef),
		"runtime" => (GenesisKind::Runtime, ObjectKind::RuntimeDef),
		_ => return Err(FederationError::InvalidUrl(format!("unknown definition kind `{kind}`"))),
	};
	let client = HomeClient::new(
		home_url,
		state.capability.allow_insecure_federation_local,
		state.capability.max_response_bytes,
		&state.capability.tls_extra_roots,
	)
	.await?;
	let summary = client.get_json::<DefinitionSummary>(&format!("/v1/{kind}s/{id}")).await?;
	if summary.id != id {
		return Err(FederationError::Verify("home returned a different definition id".to_string()));
	}
	let genesis_wire = client
		.get_bytes(&format!("/v1/objects/{}", hex_of(&summary.genesis)?))
		.await?;
	let (root, genesis_object) =
		verify::verify_genesis(&genesis_wire).map_err(|error| FederationError::Verify(error.to_string()))?;
	let genesis = Genesis::from_canonical_bytes(&genesis_object.payload_bytes)
		.map_err(|error| FederationError::Verify(error.to_string()))?;
	if genesis.kind != genesis_kind {
		return Err(FederationError::Verify("genesis is not that kind of definition".to_string()));
	}
	match state.metadata.definition(id).await.map_err(storage)? {
		Some(existing) if existing.genesis_digest != genesis_object.digest.to_vec() => {
			return Err(FederationError::Verify(
				"local definition has a different genesis".to_string(),
			));
		}
		Some(_) => {}
		None => {
			let over_quota = crate::registry::definitions::definition_quota_reached(state)
				.await
				.map_err(|(_, message)| FederationError::Rejected(message))?;
			if let Some(message) = over_quota {
				return Err(FederationError::Rejected(message));
			}
			state
				.metadata
				.put_object(&registry::stored(&genesis_object))
				.await
				.map_err(storage)?;
			state
				.metadata
				.create_definition(id, kind, &genesis_object.digest, Some(home_url), now())
				.await
				.map_err(storage)?;
		}
	}
	let current_wire = client
		.get_bytes(&format!("/v1/objects/{}", hex_of(&summary.current)?))
		.await?;
	let object = verify::verify_object(definition_kind, &current_wire, &root)
		.map_err(|error| FederationError::Verify(error.to_string()))?;
	state.metadata.put_object(&registry::stored(&object)).await.map_err(storage)?;
	state
		.metadata
		.set_definition_current(id, &object.digest)
		.await
		.map_err(storage)?;
	if genesis_kind == GenesisKind::Loader {
		sync_loader_releases(state, &client, id, &root).await?;
	}
	Ok(DefinitionReport {
		id: id.to_string(),
		kind: kind.to_string(),
		definition: object.id,
	})
}

#[derive(Serialize, Deserialize)]
struct ProjectSummary {
	project_id: String,
	genesis: String,
	#[serde(default)]
	head_seq: i64,
	#[serde(default)]
	head_entry: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct FeedPage {
	head_seq: i64,
	entries: Vec<FeedEntryView>,
}

#[derive(Serialize, Deserialize)]
struct FeedEntryView {
	seq: i64,
	kind: String,
	object: String,
	entry: String,
}

#[derive(Deserialize)]
struct SyncRequest {
	home_url: String,
	project_id: String,
}
