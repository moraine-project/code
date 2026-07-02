use std::fmt;
use std::net::IpAddr;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_crypto::ObjectKind;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::auth::AuthenticatedUser;
use crate::registry::{self, load_delegations};
use crate::routes::AppState;
use crate::verify;

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/federation/sync", post(sync_handler))
		.route("/v1/subscriptions", get(list_subscriptions))
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
					status: row.status,
					updated_at: row.updated_at,
				})
				.collect();
			Json(view).into_response()
		}
		Err(error) => storage_error(error),
	}
}

#[derive(Serialize)]
struct SubscriptionView {
	home_url: String,
	project_id: String,
	cursor_seq: i64,
	status: String,
	updated_at: i64,
}

pub async fn sync(state: &AppState, home_url: &str, project_id: &str) -> Result<SyncReport, FederationError> {
	let client = HomeClient::new(home_url, state.capability.allow_insecure_federation_local)?;
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

	let cursor = match state.metadata.subscription(home_url, project_id).await.map_err(storage)? {
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

	let page = client
		.get_json::<FeedPage>(&format!("/v1/projects/{project_id}/feed?after={cursor}&limit=100"))
		.await?;
	let mut applied = 0;
	for entry in &page.entries {
		if let Some(kind) = object_kind_for_event(&entry.kind) {
			let wire = client.get_bytes(&format!("/v1/objects/{}", hex_of(&entry.object)?)).await?;
			let delegations = load_delegations(state, project_id, &root)
				.await
				.map_err(|error| rejected(*error))?;
			let object = verify::verify_object_authorized(kind, &wire, &root, &delegations, now())
				.map_err(|error| FederationError::Verify(error.to_string()))?;
			state.metadata.put_object(&registry::stored(&object)).await.map_err(storage)?;
		}
		let entry_wire = client.get_bytes(&format!("/v1/objects/{}", hex_of(&entry.entry)?)).await?;
		registry::ingest_feed(state, project_id, &entry_wire)
			.await
			.map_err(|error| rejected(*error))?;
		applied += 1;
	}

	state
		.metadata
		.set_subscription_cursor(home_url, project_id, page.head_seq, "active", now())
		.await
		.map_err(storage)?;
	Ok(SyncReport {
		project_id: project_id.to_string(),
		applied,
		head_seq: page.head_seq,
	})
}

fn object_kind_for_event(event: &str) -> Option<ObjectKind> {
	Some(match event {
		"release-published" | "release-withdrawn" => ObjectKind::Release,
		"profile-updated" => ObjectKind::Profile,
		"key-changed" | "migration" | "recovery" => ObjectKind::Delegation,
		"advisory" => ObjectKind::Advisory,
		_ => return None,
	})
}

struct HomeClient {
	client: reqwest::Client,
	base: Url,
}

impl HomeClient {
	fn new(base: &str, allow_http_local: bool) -> Result<Self, FederationError> {
		let url = validate_home(base, allow_http_local)?;
		let client = reqwest::Client::builder()
			.timeout(Duration::from_secs(10))
			.redirect(reqwest::redirect::Policy::none())
			.build()
			.map_err(|error| FederationError::Http(error.to_string()))?;
		Ok(Self { client, base: url })
	}

	fn endpoint(&self, path: &str) -> Url {
		let mut url = self.base.clone();
		url.set_path(path.split('?').next().unwrap_or(path));
		if let Some((_, query)) = path.split_once('?') {
			url.set_query(Some(query));
		}
		url
	}

	async fn get_bytes(&self, path: &str) -> Result<Vec<u8>, FederationError> {
		let response = self
			.client
			.get(self.endpoint(path))
			.send()
			.await
			.map_err(|error| FederationError::Http(error.to_string()))?;
		if !response.status().is_success() {
			return Err(FederationError::Http(format!("{} returned {}", path, response.status())));
		}
		response
			.bytes()
			.await
			.map(|bytes| bytes.to_vec())
			.map_err(|error| FederationError::Http(error.to_string()))
	}

	async fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, FederationError> {
		let bytes = self.get_bytes(path).await?;
		serde_json::from_slice(&bytes).map_err(|error| FederationError::Decode(error.to_string()))
	}
}

fn validate_home(base: &str, allow_http_local: bool) -> Result<Url, FederationError> {
	let url = Url::parse(base).map_err(|error| FederationError::InvalidUrl(error.to_string()))?;
	match url.scheme() {
		"https" => {}
		"http" => {
			let host = url.host_str().unwrap_or_default().to_string();
			let loopback =
				host == "localhost" || host.parse::<IpAddr>().map(|address| address.is_loopback()).unwrap_or(false);
			if !(allow_http_local && loopback) {
				return Err(FederationError::InvalidUrl(
					"http is only allowed for loopback when explicitly enabled".to_string(),
				));
			}
		}
		_ => return Err(FederationError::InvalidUrl("home url must use https".to_string())),
	}
	if url.host_str().is_none() {
		return Err(FederationError::InvalidUrl("home url has no host".to_string()));
	}
	Ok(url)
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

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "subscription store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
