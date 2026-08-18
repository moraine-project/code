use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_crypto::{ObjectKind, object_id};
use moraine_model::attestation::AttestationObject;
use moraine_model::signed::{SignedObject, TrustedKey, verify_envelope};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use sqlx::Row;

use crate::auth::AuthenticatedUser;
use crate::db::{MetadataStore, StoredObject};
use crate::routes::AppState;

#[derive(Debug, Clone)]
pub struct LocationRow {
	pub url: String,
	pub kind: String,
	pub operator_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DueCommitmentRow {
	pub artifact_digest: Vec<u8>,
	pub mirror_id: String,
	pub endpoint: String,
	pub size: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct ConfirmationRow {
	pub checked_at: i64,
	pub reachable: bool,
}

#[derive(Debug, Clone)]
pub struct CommitmentRow {
	pub mirror_id: String,
	pub size: i64,
	pub accepted_at: i64,
	pub retention_until: Option<i64>,
	pub endpoint: String,
}

impl MetadataStore {
	pub async fn pin_mirror(
		&self,
		mirror_id: &str,
		public_key: &[u8],
		added_by: &str,
		added_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO mirrors (mirror_id, public_key, added_by, added_at) VALUES ($1, $2, $3, $4)
			 ON CONFLICT(mirror_id) DO UPDATE SET public_key = $2, added_by = $3, added_at = $4",
		)
		.bind(mirror_id)
		.bind(public_key)
		.bind(added_by)
		.bind(added_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn mirror_key(&self, mirror_id: &str) -> Result<Option<Vec<u8>>, sqlx::Error> {
		let row = sqlx::query("SELECT public_key FROM mirrors WHERE mirror_id = $1")
			.bind(mirror_id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.map(|row| row.get("public_key")))
	}

	pub async fn index_location(
		&self,
		artifact_digest: &[u8],
		url: &str,
		kind: &str,
		operator_id: Option<&str>,
		object_digest: &[u8],
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO locations (artifact_digest, url, kind, operator_id, object_digest) VALUES ($1, $2, $3, $4, $5)
			 ON CONFLICT (artifact_digest, url) DO UPDATE SET kind = $3, operator_id = $4, object_digest = $5",
		)
		.bind(artifact_digest)
		.bind(url)
		.bind(kind)
		.bind(operator_id)
		.bind(object_digest)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn locations_for(&self, artifact_digest: &[u8]) -> Result<Vec<LocationRow>, sqlx::Error> {
		let rows = sqlx::query("SELECT url, kind, operator_id FROM locations WHERE artifact_digest = $1 ORDER BY url")
			.bind(artifact_digest)
			.fetch_all(&self.pool)
			.await?;
		Ok(rows
			.into_iter()
			.map(|row| LocationRow {
				url: row.get("url"),
				kind: row.get("kind"),
				operator_id: row.get("operator_id"),
			})
			.collect())
	}

	pub async fn insert_commitment(
		&self,
		commitment: &CommitmentRow,
		artifact_digest: &[u8],
		object_digest: &[u8],
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO mirror_commitments (artifact_digest, mirror_id, size, accepted_at, retention_until, endpoint, object_digest)
			 VALUES ($1, $2, $3, $4, $5, $6, $7)
			 ON CONFLICT (artifact_digest, mirror_id) DO UPDATE SET size = $3, accepted_at = $4,
			 retention_until = $5, endpoint = $6, object_digest = $7",
		)
		.bind(artifact_digest)
		.bind(&commitment.mirror_id)
		.bind(commitment.size)
		.bind(commitment.accepted_at)
		.bind(commitment.retention_until)
		.bind(&commitment.endpoint)
		.bind(object_digest)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn record_confirmation(
		&self,
		artifact_digest: &[u8],
		mirror_id: &str,
		checked_at: i64,
		reachable: bool,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO mirror_confirmations (artifact_digest, mirror_id, checked_at, reachable)
			 VALUES ($1, $2, $3, $4)
			 ON CONFLICT (artifact_digest, mirror_id) DO UPDATE SET checked_at = $3, reachable = $4",
		)
		.bind(artifact_digest)
		.bind(mirror_id)
		.bind(checked_at)
		.bind(reachable)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn confirmation_for(
		&self,
		artifact_digest: &[u8],
		mirror_id: &str,
	) -> Result<Option<ConfirmationRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT checked_at, reachable FROM mirror_confirmations WHERE artifact_digest = $1 AND mirror_id = $2",
		)
		.bind(artifact_digest)
		.bind(mirror_id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| ConfirmationRow {
			checked_at: row.get("checked_at"),
			reachable: row.get::<i64, _>("reachable") != 0,
		}))
	}

	pub async fn commitments_due(&self, before: i64, limit: i64) -> Result<Vec<DueCommitmentRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT c.artifact_digest, c.mirror_id, c.endpoint, c.size
			 FROM mirror_commitments c
			 LEFT JOIN mirror_confirmations f
			   ON f.artifact_digest = c.artifact_digest AND f.mirror_id = c.mirror_id
			 WHERE f.checked_at IS NULL OR f.checked_at < $1
			 ORDER BY COALESCE(f.checked_at, 0) ASC, c.artifact_digest ASC
			 LIMIT $2",
		)
		.bind(before)
		.bind(limit)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| DueCommitmentRow {
				artifact_digest: row.get("artifact_digest"),
				mirror_id: row.get("mirror_id"),
				endpoint: row.get("endpoint"),
				size: row.get("size"),
			})
			.collect())
	}

	pub async fn commitments_for(&self, artifact_digest: &[u8]) -> Result<Vec<CommitmentRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT mirror_id, size, accepted_at, retention_until, endpoint FROM mirror_commitments WHERE artifact_digest = $1 ORDER BY accepted_at DESC",
		)
		.bind(artifact_digest)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| CommitmentRow {
				mirror_id: row.get("mirror_id"),
				size: row.get("size"),
				accepted_at: row.get("accepted_at"),
				retention_until: row.get("retention_until"),
				endpoint: row.get("endpoint"),
			})
			.collect())
	}
}

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/mirrors/{mirror_id}/keys", post(pin_mirror))
		.route("/v1/mirror-commitments", post(publish_commitment))
		.route("/v1/mirrors/{digest}", get(mirrors_for))
}

#[derive(Deserialize)]
struct MirrorKey {
	public_key: String,
}

async fn pin_mirror(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Path(mirror_id): Path<String>,
	Json(request): Json<MirrorKey>,
) -> Response {
	let mirror_id = mirror_id.trim().to_string();
	if mirror_id.is_empty() || mirror_id.len() > 128 {
		return (StatusCode::BAD_REQUEST, "invalid mirror id").into_response();
	}
	let public_key = match hex::decode(request.public_key.trim()) {
		Ok(bytes) => bytes,
		Err(_) => return (StatusCode::BAD_REQUEST, "public_key must be hex").into_response(),
	};
	if let Err(error) = TrustedKey::new(&public_key) {
		return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
	}
	if let Ok(Some(existing)) = state.metadata.mirror_key(&mirror_id).await
		&& existing != public_key
	{
		return (StatusCode::CONFLICT, "mirror already pinned with a different key").into_response();
	}
	match state.metadata.pin_mirror(&mirror_id, &public_key, &user.user_id, now()).await {
		Ok(()) => (StatusCode::CREATED, Json(serde_json::json!({ "mirror_id": mirror_id }))).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn publish_commitment(State(state): State<AppState>, body: Bytes) -> Response {
	let signed = match SignedObject::<AttestationObject>::from_bytes(&body) {
		Ok(signed) => signed,
		Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	};
	let AttestationObject::MirrorCommitment(commitment) = &signed.payload else {
		return (StatusCode::BAD_REQUEST, "object is not a mirror commitment").into_response();
	};
	let Some(public_key) = (match state.metadata.mirror_key(&commitment.mirror_id).await {
		Ok(key) => key,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::CONFLICT, "mirror key is not pinned on this instance").into_response();
	};
	let trusted = match TrustedKey::new(&public_key) {
		Ok(trusted) => trusted,
		Err(error) => return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
	};
	if verify_envelope(
		&signed.envelope,
		&signed.signed_message(ObjectKind::Attestation),
		&[trusted],
		1,
	)
	.is_err()
	{
		return (
			StatusCode::BAD_REQUEST,
			"commitment signature did not verify against the pinned key",
		)
			.into_response();
	}
	let object_digest = object_id(ObjectKind::Attestation, &signed.payload_bytes);
	let stored = StoredObject {
		digest: object_digest.to_vec(),
		kind: "attestation".to_string(),
		payload: signed.payload_bytes.clone(),
		wire: body.to_vec(),
	};
	if let Err(error) = state.metadata.put_object(&stored).await {
		return storage_error(error);
	}
	let row = CommitmentRow {
		mirror_id: commitment.mirror_id.clone(),
		size: commitment.size as i64,
		accepted_at: commitment.accepted_at,
		retention_until: commitment.retention_until,
		endpoint: commitment.endpoint.clone(),
	};
	match state
		.metadata
		.insert_commitment(&row, &commitment.artifact_digest, &object_digest)
		.await
	{
		Ok(()) => (
			StatusCode::CREATED,
			Json(serde_json::json!({ "commitment": id_for(&object_digest) })),
		)
			.into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Serialize)]
struct MirrorView {
	digest: String,
	locations: Vec<LocationView>,
	commitments: Vec<CommitmentView>,
}

#[derive(Serialize)]
struct LocationView {
	url: String,
	kind: String,
	operator_id: Option<String>,
}

#[derive(Serialize)]
struct CommitmentView {
	mirror_id: String,
	size: i64,
	accepted_at: i64,
	retention_until: Option<i64>,
	endpoint: String,
	last_checked_at: Option<i64>,
	reachable: Option<bool>,
}

async fn mirrors_for(State(state): State<AppState>, Path(digest): Path<String>) -> Response {
	let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
	let Some(bytes) = hex::decode(hex).ok().filter(|bytes| bytes.len() == 32) else {
		return (StatusCode::BAD_REQUEST, "digest must be a sha256 digest").into_response();
	};
	let locations = match state.metadata.locations_for(&bytes).await {
		Ok(locations) => locations,
		Err(error) => return storage_error(error),
	};
	let commitments = match state.metadata.commitments_for(&bytes).await {
		Ok(commitments) => commitments,
		Err(error) => return storage_error(error),
	};
	let mut confirmations = Vec::with_capacity(commitments.len());
	for commitment in &commitments {
		match state.metadata.confirmation_for(&bytes, &commitment.mirror_id).await {
			Ok(confirmation) => confirmations.push(confirmation),
			Err(error) => return storage_error(error),
		}
	}
	let view = MirrorView {
		digest: format!("sha256:{}", hex::encode(&bytes)),
		locations: locations
			.into_iter()
			.map(|location| LocationView {
				url: location.url,
				kind: location.kind,
				operator_id: location.operator_id,
			})
			.collect(),
		commitments: commitments
			.into_iter()
			.zip(confirmations)
			.map(|(commitment, confirmation)| CommitmentView {
				mirror_id: commitment.mirror_id,
				size: commitment.size,
				accepted_at: commitment.accepted_at,
				retention_until: commitment.retention_until,
				endpoint: commitment.endpoint,
				last_checked_at: confirmation.map(|confirmation| confirmation.checked_at),
				reachable: confirmation.map(|confirmation| confirmation.reachable),
			})
			.collect(),
	};
	Json(view).into_response()
}

pub async fn probe_mirrors(state: &AppState, limit: i64) -> Result<usize, String> {
	let checked_before = now() - PROBE_INTERVAL_SECONDS;
	let due = state
		.metadata
		.commitments_due(checked_before, limit)
		.await
		.map_err(|error| error.to_string())?;
	let mut confirmed = 0;
	for commitment in due {
		let reachable = confirm(&state.capability, &commitment).await;
		if reachable {
			confirmed += 1;
		}
		state
			.metadata
			.record_confirmation(&commitment.artifact_digest, &commitment.mirror_id, now(), reachable)
			.await
			.map_err(|error| error.to_string())?;
	}
	Ok(confirmed)
}

const PROBE_INTERVAL_SECONDS: i64 = 86_400;
const PROBE_TIMEOUT_SECONDS: u64 = 30;

async fn confirm(capability: &crate::capability::Capability, commitment: &DueCommitmentRow) -> bool {
	let Ok(mut base) = reqwest::Url::parse(&commitment.endpoint) else {
		return false;
	};
	if !matches!(base.scheme(), "https" | "http") {
		return false;
	}
	let insecure_http = base.scheme() == "http";
	if insecure_http && !capability.allow_insecure_federation_local {
		return false;
	}
	let Some(host) = base.host_str().map(str::to_string) else {
		return false;
	};
	let Some(port) = base.port_or_known_default() else {
		return false;
	};
	let Ok(addresses) =
		crate::federation::egress::resolve_public(&host, port, capability.allow_insecure_federation_local).await
	else {
		return false;
	};
	if insecure_http && !addresses.iter().all(std::net::IpAddr::is_loopback) {
		return false;
	}
	let Ok(client) = crate::federation::egress::pinned(
		crate::federation::egress::client_builder(&capability.tls_extra_roots),
		&host,
		port,
		&addresses,
	)
	.timeout(std::time::Duration::from_secs(PROBE_TIMEOUT_SECONDS))
	.redirect(reqwest::redirect::Policy::none())
	.build() else {
		return false;
	};
	let path = format!("/v1/blobs/sha256/{}", hex::encode(&commitment.artifact_digest));
	base.set_path(&path);
	let Ok(mut response) = client.get(base).send().await else {
		return false;
	};
	if !response.status().is_success() {
		return false;
	}
	if let Some(length) = response.content_length()
		&& length != commitment.size as u64
	{
		return false;
	}
	let mut hasher = sha2::Sha256::new();
	let mut received = 0u64;
	loop {
		match response.chunk().await {
			Ok(Some(chunk)) => {
				received += chunk.len() as u64;
				if received > commitment.size as u64 {
					return false;
				}
				hasher.update(&chunk);
			}
			Ok(None) => break,
			Err(_) => return false,
		}
	}
	received == commitment.size as u64 && hasher.finalize().as_slice() == commitment.artifact_digest.as_slice()
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
	tracing::error!(%error, "mirror store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use axum::body::{Body, to_bytes};
	use axum::http::header;
	use moraine_crypto::SigningKey;
	use moraine_model::attestation::MirrorCommitment;
	use moraine_model::signed::sign_payload;
	use tower::ServiceExt;

	use super::*;
	use crate::blob::BlobStore;
	use crate::capability::Capability;

	async fn app() -> (Router, tempfile::TempDir) {
		let (state, directory) = state().await;
		(crate::routes::router(state), directory)
	}

	async fn state() -> (AppState, tempfile::TempDir) {
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
		let state = AppState {
			store,
			metadata,
			capability: Arc::new(Capability::discover(&config)),
			login_limiter: std::sync::Arc::new(crate::auth::LoginLimiter::new()),
			metrics: std::sync::Arc::new(crate::ops::metrics::Metrics::new()),
			rate_limiter: std::sync::Arc::new(crate::auth::ratelimit::RateLimiter::new()),
			web_dir: None,
		};
		(state, directory)
	}

	async fn login(app: &Router, email: &str) -> (String, String) {
		let credentials = serde_json::json!({ "email": email, "password": "correct horse battery" }).to_string();
		let register = axum::http::Request::post("/v1/auth/register")
			.header(header::CONTENT_TYPE, "application/json")
			.body(Body::from(credentials.clone()))
			.expect("request");
		app.clone().oneshot(register).await.expect("response");
		let session = axum::http::Request::post("/v1/auth/session")
			.header(header::CONTENT_TYPE, "application/json")
			.body(Body::from(credentials))
			.expect("request");
		let response = app.clone().oneshot(session).await.expect("response");
		let token = set_cookie(&response, "moraine_session");
		let csrf = set_cookie(&response, "moraine_csrf");
		(format!("moraine_session={token}; moraine_csrf={csrf}"), csrf)
	}

	fn set_cookie(response: &Response, name: &str) -> String {
		response
			.headers()
			.get_all(header::SET_COOKIE)
			.iter()
			.find_map(|value| {
				let cookie = value.to_str().ok()?;
				cookie
					.split(';')
					.next()?
					.strip_prefix(&format!("{name}="))
					.map(str::to_string)
			})
			.unwrap_or_default()
	}

	async fn body_json(response: Response) -> serde_json::Value {
		let bytes = to_bytes(response.into_body(), 64 * 1024).await.expect("body");
		serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
	}

	#[tokio::test]
	async fn pins_a_mirror_and_records_a_commitment() {
		let (application, _directory) = app().await;
		let mirror = SigningKey::from_seed(&[21u8; 32]);
		let (cookie, csrf) = login(&application, "ops@example.org").await;

		let pin = axum::http::Request::post("/v1/mirrors/archive-one/keys")
			.header(header::CONTENT_TYPE, "application/json")
			.header(header::COOKIE, &cookie)
			.header("x-csrf-token", &csrf)
			.body(Body::from(
				serde_json::json!({ "public_key": hex::encode(mirror.verifying_key().to_bytes()) }).to_string(),
			))
			.expect("request");
		let response = application.clone().oneshot(pin).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let digest = [0x55u8; 32];
		let commitment = MirrorCommitment {
			protocol: 1,
			mirror_id: "archive-one".to_string(),
			artifact_digest: digest.to_vec(),
			size: 4096,
			accepted_at: 1_760_000_400,
			retention_until: Some(1_770_000_000),
			endpoint: "https://mirror.example".to_string(),
		};
		let signed = sign_payload(
			ObjectKind::Attestation,
			&AttestationObject::MirrorCommitment(commitment.clone()),
			&[&mirror],
		);
		let request = axum::http::Request::post("/v1/mirror-commitments")
			.body(Body::from(signed.wire_bytes()))
			.expect("request");
		let response = application.clone().oneshot(request).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let lookup = axum::http::Request::get(format!("/v1/mirrors/{}", hex::encode(digest)))
			.body(Body::empty())
			.expect("request");
		let response = application.clone().oneshot(lookup).await.expect("response");
		let view = body_json(response).await;
		assert_eq!(view["commitments"].as_array().expect("commitments").len(), 1);
		assert_eq!(view["commitments"][0]["mirror_id"], "archive-one");

		let unpinned = MirrorCommitment {
			mirror_id: "ghost".to_string(),
			..commitment
		};
		let signed = sign_payload(
			ObjectKind::Attestation,
			&AttestationObject::MirrorCommitment(unpinned),
			&[&mirror],
		);
		let request = axum::http::Request::post("/v1/mirror-commitments")
			.body(Body::from(signed.wire_bytes()))
			.expect("request");
		let response = application.oneshot(request).await.expect("response");
		assert_eq!(response.status(), StatusCode::CONFLICT);
	}

	#[tokio::test]
	async fn confirms_a_holding_only_when_the_bytes_hash_correctly() {
		let (mut state, _directory) = state().await;
		let capability = crate::capability::Capability {
			allow_insecure_federation_local: true,
			..state.capability.as_ref().clone()
		};
		state.capability = Arc::new(capability);
		let good = b"the real artifact bytes";
		let digest: [u8; 32] = sha2::Sha256::digest(good).into();
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
		let port = listener.local_addr().expect("addr").port();
		let served = good.to_vec();
		let mock = Router::new().route(
			"/v1/blobs/sha256/{digest}",
			get(move || {
				let served = served.clone();
				async move { served }
			}),
		);
		tokio::spawn(async move {
			let _ = axum::serve(listener, mock).await;
		});

		state
			.metadata
			.insert_commitment(
				&CommitmentRow {
					mirror_id: "archive-one".to_string(),
					size: good.len() as i64,
					accepted_at: now(),
					retention_until: None,
					endpoint: format!("http://127.0.0.1:{port}"),
				},
				&digest,
				&[0x99; 32],
			)
			.await
			.expect("commitment");

		assert_eq!(probe_mirrors(&state, 10).await.expect("probe"), 1);
		let confirmation = state
			.metadata
			.confirmation_for(&digest, "archive-one")
			.await
			.expect("confirmation")
			.expect("recorded");
		assert!(confirmation.reachable);
	}

	#[tokio::test]
	async fn locations_round_trip_through_the_store() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		store
			.index_location(&[0x11; 32], "https://cdn.example/a.jar", "origin", None, &[0x22; 32])
			.await
			.expect("index");
		let locations = store.locations_for(&[0x11; 32]).await.expect("locations");
		assert_eq!(locations.len(), 1);
		assert_eq!(locations[0].url, "https://cdn.example/a.jar");
		assert_eq!(locations[0].kind, "origin");
	}
}
