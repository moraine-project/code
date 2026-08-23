pub mod egress;
pub mod home;
pub mod mirrors;
pub mod notifications;
pub mod subscriptions;
pub mod webhooks;

use std::fmt;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
pub(crate) use home::HomeClient;
pub use mirrors::probe_mirrors;
use moraine_crypto::ObjectKind;
use moraine_model::Canonical;
use moraine_model::genesis::{Genesis, GenesisKind};
use serde::{Deserialize, Serialize};

use crate::auth::AuthenticatedUser;
use crate::registry::{self, load_delegations};
use crate::routes::AppState;
use crate::verify;

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/federation/sync", post(sync_handler))
		.route("/v1/federation/resync", post(resync_handler))
		.route("/v1/federation/sync-definition", post(sync_definition_handler))
		.route("/v1/federation/subscribe-definition", post(subscribe_definition_handler))
		.route("/v1/definition-subscriptions", get(list_definition_subscriptions))
		.route("/v1/subscriptions", get(list_subscriptions).delete(unsubscribe))
		.route("/v1/subscriptions/reset", post(reset_subscription))
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

#[derive(Serialize)]
pub struct ResyncReport {
	pub synced: usize,
	pub failed: usize,
}

pub async fn resync_subscriptions(state: &AppState) -> Result<ResyncReport, FederationError> {
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
			match sync(&state, &subscription.home_url, &subscription.project_id).await {
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
	Ok(ResyncReport { synced, failed })
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
			state
				.metadata
				.put_object(&registry::stored(&genesis_object))
				.await
				.map_err(storage)?;
			state
				.metadata
				.create_definition(id, kind, &genesis_object.digest, now())
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

#[derive(Debug)]
pub enum FederationError {
	InvalidUrl(String),
	Http(String),
	Decode(String),
	Verify(String),
	Storage(String),
	Rejected(String),
}

impl fmt::Display for FederationError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::InvalidUrl(detail) => write!(f, "invalid home url: {detail}"),
			Self::Http(detail) => write!(f, "home request failed: {detail}"),
			Self::Decode(detail) => write!(f, "malformed home response: {detail}"),
			Self::Verify(detail) => write!(f, "home data did not verify: {detail}"),
			Self::Storage(detail) => write!(f, "local storage failed: {detail}"),
			Self::Rejected(detail) => write!(f, "home entry rejected: {detail}"),
		}
	}
}

#[derive(Serialize, Deserialize)]
struct ProjectSummary {
	project_id: String,
	genesis: String,
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

#[derive(Serialize)]
pub struct SyncReport {
	project_id: String,
	applied: usize,
	head_seq: i64,
}

async fn sync_handler(State(state): State<AppState>, user: AuthenticatedUser, Json(request): Json<SyncRequest>) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let home_url = request.home_url.trim_end_matches('/').to_string();
	match sync(&state, &home_url, &request.project_id).await {
		Ok(report) => Json(report).into_response(),
		Err(error) => {
			let status = match error {
				FederationError::InvalidUrl(_) => StatusCode::BAD_REQUEST,
				FederationError::Rejected(_) => StatusCode::CONFLICT,
				FederationError::Verify(_) | FederationError::Decode(_) => StatusCode::BAD_GATEWAY,
				_ => StatusCode::BAD_GATEWAY,
			};
			state.metrics.record_federation_failure(&error);
			(status, error.to_string()).into_response()
		}
	}
}

async fn list_subscriptions(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	match state.metadata.subscriptions().await {
		Ok(rows) => {
			let view: Vec<SubscriptionView> = rows
				.into_iter()
				.map(|row| SubscriptionView {
					home_url: row.home_url,
					project_id: row.project_id,
					cursor_seq: row.cursor_seq,
					lag_entries: (row.remote_head_seq - row.cursor_seq).max(0),
					resets: row.reset_count,
					status: row.status,
					updated_at: row.updated_at,
				})
				.collect();
			Json(view).into_response()
		}
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
struct ResetQuery {
	home_url: String,
	project_id: String,
	#[serde(default)]
	cursor: Option<i64>,
}

async fn reset_subscription(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Query(query): Query<ResetQuery>,
) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let cursor = query.cursor.unwrap_or(0).max(0);
	match state
		.metadata
		.reset_subscription(&query.home_url, &query.project_id, cursor, now())
		.await
	{
		Ok(true) => Json(serde_json::json!({ "cursor_seq": cursor })).into_response(),
		Ok(false) => (StatusCode::NOT_FOUND, "not subscribed to that project").into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
struct UnsubscribeQuery {
	home_url: String,
	project_id: String,
}

async fn unsubscribe(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Query(query): Query<UnsubscribeQuery>,
) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	match state.metadata.remove_subscription(&query.home_url, &query.project_id).await {
		Ok(true) => StatusCode::NO_CONTENT.into_response(),
		Ok(false) => (StatusCode::NOT_FOUND, "not subscribed to that project").into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Serialize)]
struct SubscriptionView {
	home_url: String,
	project_id: String,
	cursor_seq: i64,
	lag_entries: i64,
	resets: i64,
	status: String,
	updated_at: i64,
}

pub async fn sync(state: &AppState, home_url: &str, project_id: &str) -> Result<SyncReport, FederationError> {
	let client = HomeClient::new(
		home_url,
		state.capability.allow_insecure_federation_local,
		state.capability.max_response_bytes,
		&state.capability.tls_extra_roots,
	)
	.await?;
	let summary = client
		.get_json::<ProjectSummary>(&format!("/v1/projects/{project_id}"))
		.await?;
	if summary.project_id != project_id {
		return Err(FederationError::Verify("home returned a different project id".to_string()));
	}

	let genesis_wire = client
		.get_bytes(&format!("/v1/objects/{}", hex_of(&summary.genesis)?))
		.await?;
	let (root, genesis) =
		verify::verify_genesis(&genesis_wire).map_err(|error| FederationError::Verify(error.to_string()))?;
	if genesis.id != project_id {
		return Err(FederationError::Verify(
			"genesis id does not match the requested project".to_string(),
		));
	}

	let mut cursor = match state.metadata.subscription(home_url, project_id).await.map_err(storage)? {
		Some(subscription) => subscription.cursor_seq,
		None => 0,
	};
	state
		.metadata
		.upsert_subscription(home_url, project_id, "active", now())
		.await
		.map_err(storage)?;

	match state.metadata.project(project_id).await.map_err(storage)? {
		Some(existing) if existing.genesis_digest != genesis.digest.to_vec() => {
			return Err(FederationError::Verify("local project has a different genesis".to_string()));
		}
		Some(_) => {}
		None => {
			state
				.metadata
				.put_object(&registry::stored(&genesis))
				.await
				.map_err(storage)?;
			state
				.metadata
				.create_project(project_id, &genesis.digest)
				.await
				.map_err(storage)?;
		}
	}

	let mut applied = 0;
	let mut head_seq;
	let mut pages = 0;
	loop {
		let page = client
			.get_json::<FeedPage>(&format!("/v1/projects/{project_id}/feed?after={cursor}&limit=100"))
			.await?;
		head_seq = page.head_seq;
		if head_seq < cursor {
			return Err(FederationError::Rejected(format!(
				"the home's feed went backwards from {cursor} to {head_seq}"
			)));
		}
		let Some(last) = page.entries.last() else {
			break;
		};
		for entry in &page.entries {
			if let Some(kind) = object_kind_for_event(&entry.kind) {
				let wire = client.get_bytes(&format!("/v1/objects/{}", hex_of(&entry.object)?)).await?;
				let delegations = load_delegations(state, project_id, &root)
					.await
					.map_err(|error| rejected(*error))?;
				let object = verify::verify_object_authorized(kind, &wire, &root, &delegations, now())
					.map_err(|error| FederationError::Verify(error.to_string()))?;
				store_synced_object(state, &object).await?;
				if kind == ObjectKind::Release
					&& let Some(digest) = release_changelog_digest(&object.payload_bytes)
				{
					let wire = client.get_bytes(&format!("/v1/objects/{}", hex::encode(digest))).await?;
					let changelog =
						verify::verify_object_authorized(ObjectKind::Changelog, &wire, &root, &delegations, now())
							.map_err(|error| FederationError::Verify(error.to_string()))?;
					store_synced_object(state, &changelog).await?;
				}
			}
			let entry_wire = client.get_bytes(&format!("/v1/objects/{}", hex_of(&entry.entry)?)).await?;
			registry::ingest_feed(state, project_id, &entry_wire)
				.await
				.map_err(|error| rejected(*error))?;
			applied += 1;
		}
		if last.seq <= cursor {
			break;
		}
		state
			.metadata
			.set_subscription_cursor(home_url, project_id, last.seq, head_seq, "active", now())
			.await
			.map_err(storage)?;
		cursor = last.seq;
		if last.seq >= head_seq {
			break;
		}
		pages += 1;
		if pages >= state.capability.max_sync_pages as usize {
			return Err(FederationError::Verify(
				"the home feed is longer than one sync will follow".to_string(),
			));
		}
	}

	Ok(SyncReport {
		project_id: project_id.to_string(),
		applied,
		head_seq,
	})
}

fn release_changelog_digest(payload: &[u8]) -> Option<Vec<u8>> {
	let moraine_model::release::ReleaseObject::Release(release) =
		moraine_model::release::ReleaseObject::from_canonical_bytes(payload).ok()?
	else {
		return None;
	};
	release.changelog_digest
}

fn object_kind_for_event(event: &str) -> Option<ObjectKind> {
	Some(match event {
		"release-published" | "release-withdrawn" => ObjectKind::Release,
		"profile-updated" => ObjectKind::Profile,
		"key-changed" | "migration" | "recovery" | "ownership-transferred" => ObjectKind::Delegation,
		"advisory" => ObjectKind::Advisory,
		_ => return None,
	})
}

async fn sync_loader_releases(
	state: &AppState,
	client: &HomeClient,
	loader_id: &str,
	root: &moraine_model::trust::RootSet,
) -> Result<(), FederationError> {
	let releases = client
		.get_json::<Vec<LoaderReleaseSummary>>(&format!("/v1/loaders/{loader_id}/releases"))
		.await?;
	for release in releases {
		let wire = client
			.get_bytes(&format!("/v1/objects/{}", hex_of(&release.release)?))
			.await?;
		let object = verify::verify_object(ObjectKind::LoaderDef, &wire, root)
			.map_err(|error| FederationError::Verify(error.to_string()))?;
		state.metadata.put_object(&registry::stored(&object)).await.map_err(storage)?;
		let Ok(moraine_model::definition::LoaderObject::Release(payload)) =
			moraine_model::definition::LoaderObject::from_canonical_bytes(&object.payload_bytes)
		else {
			continue;
		};
		state
			.metadata
			.index_loader_release(&payload.loader_id, &payload.version_id, &object.digest, payload.declared_time)
			.await
			.map_err(storage)?;
	}
	Ok(())
}

async fn store_synced_object(state: &AppState, object: &verify::VerifiedObject) -> Result<(), FederationError> {
	registry::store_object_record(state, object)
		.await
		.map_err(|error| match error {
			sqlx::Error::Protocol(message) => FederationError::Verify(message),
			other => storage(other),
		})
}

fn storage(error: sqlx::Error) -> FederationError {
	FederationError::Storage(error.to_string())
}

fn rejected(error: Response) -> FederationError {
	let status = error.status();
	FederationError::Rejected(format!("local ingest returned {}", status))
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

fn hex_of(id: &str) -> Result<String, FederationError> {
	let hex = id
		.strip_prefix("gd:sha256:")
		.ok_or_else(|| FederationError::Decode(format!("`{id}` is not an object id")))?;
	if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
		return Err(FederationError::Decode(format!("`{id}` is not an object id")));
	}
	Ok(hex.to_string())
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "subscription store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}

#[cfg(test)]
#[path = "federation_tests.rs"]
mod tests;
