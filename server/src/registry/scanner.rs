use std::path::PathBuf;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_crypto::{ObjectKind, SigningKey, object_id};
use moraine_model::attestation::{Attestation, AttestationKind, AttestationObject};
use moraine_model::signed::{SignedObject, TrustedKey, sign_payload, verify_envelope};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tokio::io::AsyncWriteExt;

use crate::auth::AuthenticatedUser;
use crate::config::Config;
use crate::db::StoredObject;
use crate::routes::AppState;

mod adapter;
mod store;

use adapter::adapter_for;

#[derive(Debug, Clone)]
pub struct Provider {
	pub id: String,
	pub kind: String,
	pub command: String,
	pub args: Vec<String>,
	pub public_key: Vec<u8>,
	pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct Job {
	pub id: String,
	pub provider_id: String,
	pub artifact_digest: Vec<u8>,
	pub status: String,
	pub requested_by: String,
	pub attempts: i64,
	pub result_json: Option<String>,
	pub error: Option<String>,
}

fn new_id() -> String {
	let mut bytes = [0u8; 16];
	getrandom::fill(&mut bytes).expect("system randomness");
	hex::encode(bytes)
}

#[derive(Deserialize)]
struct ProviderRequest {
	provider_id: String,
	kind: String,
	command: String,
	#[serde(default)]
	args: Vec<String>,
	public_key: String,
	#[serde(default = "default_true")]
	enabled: bool,
}

#[derive(Deserialize)]
struct ScanRequest {
	artifact_digest: String,
	provider_id: Option<String>,
}

#[derive(Deserialize)]
struct PolicyRequest {
	id: String,
	provider_id: String,
	#[serde(default = "default_true")]
	enabled: bool,
	#[serde(default = "default_true")]
	auto_scan: bool,
}

#[derive(Deserialize)]
struct SubscriptionRequest {
	provider_id: String,
	endpoint: String,
	#[serde(default = "default_interval")]
	interval_seconds: i64,
}

#[derive(Serialize)]
struct ProviderView {
	provider_id: String,
	kind: String,
	command: String,
	args: Vec<String>,
	public_key: String,
	enabled: bool,
}

#[derive(Serialize)]
struct JobView {
	id: String,
	provider_id: String,
	artifact_digest: String,
	status: String,
	requested_by: String,
	attempts: i64,
	result: Option<serde_json::Value>,
	error: Option<String>,
}

fn default_true() -> bool {
	true
}
fn default_interval() -> i64 {
	3600
}
fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|d| d.as_secs() as i64)
		.unwrap_or(0)
}

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/scanners", get(list_providers).post(create_provider))
		.route("/v1/scans", get(list_jobs).post(request_scan))
		.route("/v1/scans/{id}/rescan", post(rescan))
		.route("/v1/scanner-policies", get(list_policies).post(put_policy))
		.route("/v1/scanner-subscriptions", get(list_subscriptions).post(create_subscription))
}

fn operator(user: &AuthenticatedUser) -> Option<Response> {
	if user.allows("directory:manage") {
		None
	} else {
		Some((StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response())
	}
}

async fn list_providers(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if let Some(response) = operator(&user) {
		return response;
	}
	match state.metadata.scanner_providers().await {
		Ok(items) => Json(
			items
				.into_iter()
				.map(|item| ProviderView {
					provider_id: item.id,
					kind: item.kind,
					command: item.command,
					args: item.args,
					public_key: hex::encode(item.public_key),
					enabled: item.enabled,
				})
				.collect::<Vec<_>>(),
		)
		.into_response(),
		Err(error) => storage_error(error),
	}
}

async fn create_provider(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<ProviderRequest>,
) -> Response {
	if let Some(response) = operator(&user) {
		return response;
	}
	let Ok(public_key) = hex::decode(request.public_key.trim()) else {
		return (StatusCode::BAD_REQUEST, "public_key must be hex").into_response();
	};
	if TrustedKey::new(&public_key).is_err() {
		return (StatusCode::BAD_REQUEST, "public_key must be a valid Ed25519 key").into_response();
	}
	if request.provider_id.trim().is_empty() || request.command.trim().is_empty() || request.provider_id.len() > 128 {
		return (StatusCode::BAD_REQUEST, "invalid scanner provider").into_response();
	}
	let provider = Provider {
		id: request.provider_id.trim().to_string(),
		kind: request.kind.trim().to_string(),
		command: request.command,
		args: request.args,
		public_key,
		enabled: request.enabled,
	};
	match state.metadata.put_scanner_provider(&provider, now()).await {
		Ok(()) => (
			StatusCode::CREATED,
			Json(ProviderView {
				provider_id: provider.id,
				kind: provider.kind,
				command: provider.command,
				args: provider.args,
				public_key: hex::encode(provider.public_key),
				enabled: provider.enabled,
			}),
		)
			.into_response(),
		Err(error) => storage_error(error),
	}
}

async fn request_scan(State(state): State<AppState>, user: AuthenticatedUser, Json(request): Json<ScanRequest>) -> Response {
	if let Some(response) = operator(&user) {
		return response;
	}
	let digest = match parse_digest(&request.artifact_digest) {
		Ok(digest) => digest,
		Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
	};
	if state.store.size(&digest).await.ok().flatten().is_none() {
		return (StatusCode::NOT_FOUND, "artifact not found").into_response();
	}
	let provider = match request.provider_id.or_else(|| Some("local-clamav".to_string())) {
		Some(id) => id,
		None => unreachable!(),
	};
	match state.metadata.scanner_provider(&provider).await {
		Ok(Some(provider)) if provider.enabled => {}
		Ok(Some(_)) => return (StatusCode::CONFLICT, "scanner provider is disabled").into_response(),
		Ok(None) => return (StatusCode::NOT_FOUND, "scanner provider not found").into_response(),
		Err(error) => return storage_error(error),
	}
	match state.metadata.enqueue_scan(&provider, &digest, &user.user_id, now()).await {
		Ok(id) => (StatusCode::ACCEPTED, Json(serde_json::json!({"id": id}))).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn list_jobs(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if let Some(response) = operator(&user) {
		return response;
	}
	match state.metadata.scan_jobs(100).await {
		Ok(items) => Json(items.into_iter().map(job_view).collect::<Vec<_>>()).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn rescan(State(state): State<AppState>, user: AuthenticatedUser, Path(id): Path<String>) -> Response {
	if let Some(response) = operator(&user) {
		return response;
	}
	let job = match state
		.metadata
		.scan_jobs(1000)
		.await
		.ok()
		.and_then(|jobs| jobs.into_iter().find(|job| job.id == id))
	{
		Some(job) => job,
		None => return StatusCode::NOT_FOUND.into_response(),
	};
	match state
		.metadata
		.enqueue_scan(&job.provider_id, &job.artifact_digest, &user.user_id, now())
		.await
	{
		Ok(new_id) => (StatusCode::ACCEPTED, Json(serde_json::json!({"id": new_id}))).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn list_policies(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if let Some(response) = operator(&user) {
		return response;
	}
	match state.metadata.scanner_policies().await { Ok(items) => Json(items.into_iter().map(|(id, provider_id, enabled, auto_scan)| serde_json::json!({"id": id, "provider_id": provider_id, "enabled": enabled, "auto_scan": auto_scan})).collect::<Vec<_>>()).into_response(), Err(error) => storage_error(error) }
}

async fn put_policy(State(state): State<AppState>, user: AuthenticatedUser, Json(request): Json<PolicyRequest>) -> Response {
	if let Some(response) = operator(&user) {
		return response;
	}
	if request.id.trim().is_empty() {
		return (StatusCode::BAD_REQUEST, "policy id is required").into_response();
	}
	match state
		.metadata
		.put_scanner_policy(
			request.id.trim(),
			request.provider_id.trim(),
			request.enabled,
			request.auto_scan,
			now(),
		)
		.await
	{
		Ok(()) => StatusCode::NO_CONTENT.into_response(),
		Err(error) => storage_error(error),
	}
}

async fn list_subscriptions(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if let Some(response) = operator(&user) {
		return response;
	}
	match sqlx::query("SELECT id, provider_id, endpoint, interval_seconds, enabled, last_polled_at FROM scanner_subscriptions ORDER BY id").fetch_all(&state.metadata.pool).await {
		Ok(rows) => Json(rows.into_iter().map(|row| serde_json::json!({"id": row.get::<String, _>("id"), "provider_id": row.get::<String, _>("provider_id"), "endpoint": row.get::<String, _>("endpoint"), "interval_seconds": row.get::<i64, _>("interval_seconds"), "enabled": row.get::<i64, _>("enabled") != 0, "last_polled_at": row.get::<Option<i64>, _>("last_polled_at")})).collect::<Vec<_>>()).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn create_subscription(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<SubscriptionRequest>,
) -> Response {
	if let Some(response) = operator(&user) {
		return response;
	}
	let Ok(url) = reqwest::Url::parse(request.endpoint.trim()) else {
		return (StatusCode::BAD_REQUEST, "endpoint must be an absolute URL").into_response();
	};
	if url.scheme() != "https" {
		return (StatusCode::BAD_REQUEST, "scanner subscriptions require HTTPS").into_response();
	}
	let id = new_id();
	let result = sqlx::query(
		"INSERT INTO scanner_subscriptions (id, provider_id, endpoint, interval_seconds, created_at) VALUES ($1, $2, $3, $4, $5)",
	)
	.bind(&id)
	.bind(request.provider_id.trim())
	.bind(url.as_str())
	.bind(request.interval_seconds.clamp(60, 86_400))
	.bind(now())
	.execute(&state.metadata.pool)
	.await;
	match result {
		Ok(_) => (StatusCode::CREATED, Json(serde_json::json!({"id": id}))).into_response(),
		Err(error) => storage_error(error),
	}
}

fn job_view(job: Job) -> JobView {
	JobView {
		id: job.id,
		provider_id: job.provider_id,
		artifact_digest: format!("sha256:{}", hex::encode(job.artifact_digest)),
		status: job.status,
		requested_by: job.requested_by,
		attempts: job.attempts,
		result: job.result_json.and_then(|value| serde_json::from_str(&value).ok()),
		error: job.error,
	}
}

fn parse_digest(value: &str) -> Result<[u8; 32], &'static str> {
	let value = value.trim().strip_prefix("sha256:").unwrap_or(value.trim());
	let bytes = hex::decode(value).map_err(|_| "artifact_digest must be sha256 hex")?;
	bytes.try_into().map_err(|_| "artifact_digest must be 32 bytes")
}

pub async fn run_worker(state: AppState, config: Config) {
	if !config.scanner_enabled {
		return;
	}
	let key = match load_or_create_key(&config.data_dir.join("scanner.key")) {
		Some(key) => key,
		None => {
			tracing::error!("scanner enabled but provider signing key could not be loaded");
			return;
		}
	};
	let (command, args) = local_command(&config);
	let provider = Provider {
		id: config.scanner_provider_id.clone(),
		kind: config.scanner_kind.clone(),
		command,
		args,
		public_key: key.verifying_key().to_bytes().to_vec(),
		enabled: true,
	};
	if let Err(error) = state.metadata.put_scanner_provider(&provider, now()).await {
		tracing::error!(%error, "could not register local scanner provider");
		return;
	}
	loop {
		if let Ok(items) = state.metadata.auto_scan_digests().await {
			for (provider_id, digest) in items {
				let _ = state.metadata.enqueue_scan(&provider_id, &digest, "policy", now()).await;
			}
		}
		poll_subscriptions(&state).await;
		if let Ok(Some(job)) = state.metadata.claim_scan_job(now()).await {
			execute_job(&state, &config, &key, job).await;
		}
		tokio::time::sleep(Duration::from_secs(5)).await;
	}
}

fn local_command(config: &Config) -> (String, Vec<String>) {
	match config.scanner_kind.as_str() {
		"clamav" => ("clamscan".to_string(), vec!["--no-summary".to_string()]),
		_ => (config.scanner_command.clone(), config.scanner_args.clone()),
	}
}

async fn poll_subscriptions(state: &AppState) {
	let rows = match sqlx::query(
		"SELECT id, provider_id, endpoint, interval_seconds, last_polled_at FROM scanner_subscriptions WHERE enabled = 1",
	)
	.fetch_all(&state.metadata.pool)
	.await
	{
		Ok(rows) => rows,
		Err(error) => {
			tracing::warn!(%error, "scanner subscription lookup failed");
			return;
		}
	};
	for row in rows {
		let id: String = row.get("id");
		let provider_id: String = row.get("provider_id");
		let endpoint: String = row.get("endpoint");
		let interval: i64 = row.get("interval_seconds");
		let last: Option<i64> = row.get("last_polled_at");
		if last.is_some_and(|last| now() - last < interval) {
			continue;
		}
		let result = async {
			let body = reqwest::Client::new()
				.get(&endpoint)
				.send()
				.await
				.map_err(|error| error.to_string())?
				.error_for_status()
				.map_err(|error| error.to_string())?
				.text()
				.await
				.map_err(|error| error.to_string())?;
			let value: serde_json::Value = serde_json::from_str(&body).map_err(|error| error.to_string())?;
			let records = value
				.get("records")
				.and_then(serde_json::Value::as_array)
				.or_else(|| value.as_array())
				.ok_or_else(|| "subscription response must be an array or {records: []}".to_string())?;
			let provider = state
				.metadata
				.scanner_provider(&provider_id)
				.await
				.map_err(|error| error.to_string())?
				.ok_or_else(|| "subscription provider is not registered".to_string())?;
			let trusted = TrustedKey::new(&provider.public_key).map_err(|error| error.to_string())?;
			let mut imported = 0;
			for record in records {
				let Some(hex_wire) = record.as_str() else {
					continue;
				};
				let wire = hex::decode(hex_wire).map_err(|_| "subscription record is not hex".to_string())?;
				let signed = SignedObject::<AttestationObject>::from_bytes(&wire).map_err(|error| error.to_string())?;
				if verify_envelope(
					&signed.envelope,
					&signed.signed_message(ObjectKind::Attestation),
					&[trusted],
					1,
				)
				.is_err()
				{
					continue;
				}
				let AttestationObject::Evidence(attestation) = &signed.payload else {
					continue;
				};
				if attestation.signer_id != provider_id {
					continue;
				}
				let digest = object_id(ObjectKind::Attestation, &signed.payload_bytes);
				state
					.metadata
					.put_object(&StoredObject {
						digest: digest.to_vec(),
						kind: "attestation".to_string(),
						payload: signed.payload_bytes.clone(),
						wire,
					})
					.await
					.map_err(|error| error.to_string())?;
				state
					.metadata
					.insert_attestation(attestation, &digest)
					.await
					.map_err(|error| error.to_string())?;
				imported += 1;
			}
			Ok::<usize, String>(imported)
		}
		.await;
		match result {
			Ok(imported) => {
				let _ = sqlx::query("UPDATE scanner_subscriptions SET last_polled_at = $2 WHERE id = $1")
					.bind(&id)
					.bind(now())
					.execute(&state.metadata.pool)
					.await;
				if imported > 0 {
					tracing::info!(provider = %provider_id, imported, "imported scanner attestations");
				}
			}
			Err(error) => tracing::warn!(provider = %provider_id, %error, "scanner subscription poll failed"),
		}
	}
}

async fn execute_job(state: &AppState, config: &Config, key: &SigningKey, job: Job) {
	let Some(provider) = state.metadata.scanner_provider(&job.provider_id).await.ok().flatten() else {
		let _ = state
			.metadata
			.finish_scan_job(&job.id, "failed", None, Some("scanner provider is not configured"), now())
			.await;
		return;
	};
	let Ok(digest): Result<[u8; 32], _> = job.artifact_digest.as_slice().try_into() else {
		finish_error(state, &job.id, "invalid artifact digest".to_string()).await;
		return;
	};
	let Some(mut stream) = state.store.read(&digest, None).await.ok().flatten() else {
		let _ = state
			.metadata
			.finish_scan_job(&job.id, "failed", None, Some("artifact not found"), now())
			.await;
		return;
	};
	let temp = config.data_dir.join(format!("scanner-{}.artifact", job.id));
	let mut file = match tokio::fs::File::create(&temp).await {
		Ok(file) => file,
		Err(error) => {
			finish_error(state, &job.id, error.to_string()).await;
			return;
		}
	};
	use futures_util::StreamExt;
	while let Some(chunk) = stream.next().await {
		match chunk {
			Ok(bytes) => {
				if file.write_all(&bytes).await.is_err() {
					finish_error(state, &job.id, "artifact write failed".to_string()).await;
					return;
				}
			}
			Err(error) => {
				finish_error(state, &job.id, error.to_string()).await;
				return;
			}
		}
	}
	drop(file);
	let output = tokio::time::timeout(
		Duration::from_secs(config.scanner_timeout_seconds),
		adapter_for(provider.clone()).scan(&temp),
	)
	.await;
	let _ = tokio::fs::remove_file(&temp).await;
	let (status, result, error) = match output {
		Err(_) => ("failed", None, Some("scanner timed out".to_string())),
		Ok(Err(error)) => ("failed", None, Some(error.to_string())),
		Ok(Ok(output)) => {
			let successful = matches!(output.verdict.as_str(), "clean" | "finding");
			(
				if successful { "succeeded" } else { "failed" },
				Some(serde_json::to_value(output).unwrap_or_else(|_| serde_json::json!({"verdict": "error"}))),
				if successful {
					None
				} else {
					Some("scanner returned an execution error".to_string())
				},
			)
		}
	};
	if let Some(result) = result.as_ref()
		&& status == "succeeded"
	{
		let _ = publish_result(state, key, &provider.id, &job.artifact_digest, result).await;
	}
	let result_text = result.as_ref().map(serde_json::Value::to_string);
	let _ = state
		.metadata
		.finish_scan_job(&job.id, status, result_text.as_deref(), error.as_deref(), now())
		.await;
}

async fn publish_result(
	state: &AppState,
	key: &SigningKey,
	provider_id: &str,
	digest: &[u8],
	result: &serde_json::Value,
) -> Result<(), String> {
	let attestation = Attestation {
		protocol: 1,
		artifact_digest: digest.to_vec(),
		subject_kind: "release".to_string(),
		subject_id: format!("sha256:{}", hex::encode(digest)),
		kind: AttestationKind::ScannerResult,
		media_type: "application/vnd.moraine.scanner-result+json".to_string(),
		body_digest: None,
		body_inline: Some(result.to_string().into_bytes()),
		signer_id: provider_id.to_string(),
		issued_at: now(),
	};
	let object = AttestationObject::Evidence(attestation.clone());
	let signed = sign_payload(ObjectKind::Attestation, &object, &[key]);
	let object_digest = object_id(ObjectKind::Attestation, &signed.payload_bytes);
	let wire = signed.wire_bytes();
	state
		.metadata
		.put_object(&StoredObject {
			digest: object_digest.to_vec(),
			kind: "attestation".to_string(),
			payload: signed.payload_bytes,
			wire,
		})
		.await
		.map_err(|error| error.to_string())?;
	state
		.metadata
		.insert_attestation(&attestation, &object_digest)
		.await
		.map_err(|error| error.to_string())
}

async fn finish_error(state: &AppState, id: &str, error: String) {
	let _ = state.metadata.finish_scan_job(id, "failed", None, Some(&error), now()).await;
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "scanner storage error");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}

fn load_or_create_key(path: &PathBuf) -> Option<SigningKey> {
	if let Ok(text) = std::fs::read_to_string(path) {
		return Some(SigningKey::from_seed(
			&<[u8; 32]>::try_from(hex::decode(text.trim()).ok()?.as_slice()).ok()?,
		));
	}
	let mut seed = [0u8; 32];
	getrandom::fill(&mut seed).ok()?;
	std::fs::create_dir_all(path.parent()?).ok()?;
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		let mut options = std::fs::OpenOptions::new();
		options.write(true).create_new(true).mode(0o600);
		use std::io::Write;
		let mut file = options.open(path).ok()?;
		file.write_all(hex::encode(seed).as_bytes()).ok()?;
	}
	#[cfg(not(unix))]
	{
		std::fs::write(path, hex::encode(seed)).ok()?;
	}
	Some(SigningKey::from_seed(&seed))
}
