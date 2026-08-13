use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::Canonical;
use moraine_model::event::{Event, EventKind};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::auth::AuthenticatedUser;
use crate::routes::AppState;
use crate::store::MetadataStore;

const MAX_ATTEMPTS: i64 = 8;
const BACKOFF_BASE_SECONDS: i64 = 30;

#[derive(Debug, Clone)]
pub struct WebhookRow {
	pub id: String,
	pub url: String,
	pub event_kinds: String,
	pub created_at: i64,
}

struct DeliveryRow {
	id: String,
	url: String,
	body: String,
	attempt: i64,
}

impl MetadataStore {
	pub async fn create_webhook(
		&self,
		id: &str,
		owner_id: &str,
		url: &str,
		event_kinds: &str,
		created_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT INTO webhooks (id, owner_id, url, event_kinds, created_at) VALUES ($1, $2, $3, $4, $5)")
			.bind(id)
			.bind(owner_id)
			.bind(url)
			.bind(event_kinds)
			.bind(created_at)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn webhooks_for_owner(&self, owner_id: &str) -> Result<Vec<WebhookRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, url, event_kinds, created_at FROM webhooks WHERE owner_id = $1 AND revoked_at IS NULL ORDER BY created_at",
		)
		.bind(owner_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(webhook_from_row).collect())
	}

	pub async fn active_webhooks(&self) -> Result<Vec<WebhookRow>, sqlx::Error> {
		let rows = sqlx::query("SELECT id, url, event_kinds, created_at FROM webhooks WHERE revoked_at IS NULL")
			.fetch_all(&self.pool)
			.await?;
		Ok(rows.into_iter().map(webhook_from_row).collect())
	}

	pub async fn revoke_webhook(&self, owner_id: &str, id: &str, revoked_at: i64) -> Result<bool, sqlx::Error> {
		let result =
			sqlx::query("UPDATE webhooks SET revoked_at = $1 WHERE id = $2 AND owner_id = $3 AND revoked_at IS NULL")
				.bind(revoked_at)
				.bind(id)
				.bind(owner_id)
				.execute(&self.pool)
				.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn enqueue_delivery(
		&self,
		id: &str,
		webhook_id: &str,
		event_id: &str,
		url: &str,
		body: &str,
		next_attempt_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO webhook_deliveries (id, webhook_id, event_id, url, body, status, next_attempt_at, created_at)
			 VALUES ($1, $2, $3, $4, $5, 'pending', $6, $6) ON CONFLICT DO NOTHING",
		)
		.bind(id)
		.bind(webhook_id)
		.bind(event_id)
		.bind(url)
		.bind(body)
		.bind(next_attempt_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	async fn due_deliveries(&self, now: i64, limit: i64) -> Result<Vec<DeliveryRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, url, body, attempt FROM webhook_deliveries WHERE status = 'pending' AND next_attempt_at <= $1 ORDER BY next_attempt_at LIMIT $2",
		)
		.bind(now)
		.bind(limit)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| DeliveryRow {
				id: row.get("id"),
				url: row.get("url"),
				body: row.get("body"),
				attempt: row.get("attempt"),
			})
			.collect())
	}

	pub async fn prune_deliveries(&self, before: i64) -> Result<u64, sqlx::Error> {
		let result = sqlx::query("DELETE FROM webhook_deliveries WHERE created_at < $1")
			.bind(before)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected())
	}

	async fn finish_delivery(
		&self,
		id: &str,
		attempt: i64,
		status: &str,
		next_attempt_at: i64,
		delivered_at: Option<i64>,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"UPDATE webhook_deliveries SET attempt = $1, status = $2, next_attempt_at = $3, delivered_at = $4 WHERE id = $5",
		)
		.bind(attempt)
		.bind(status)
		.bind(next_attempt_at)
		.bind(delivered_at)
		.bind(id)
		.execute(&self.pool)
		.await?;
		Ok(())
	}
}

fn webhook_from_row(row: sqlx::any::AnyRow) -> WebhookRow {
	WebhookRow {
		id: row.get("id"),
		url: row.get("url"),
		event_kinds: row.get("event_kinds"),
		created_at: row.get("created_at"),
	}
}

pub(crate) async fn enqueue_event(
	state: &AppState,
	event_kind: &str,
	project_id: &str,
	object_digest: &[u8],
	feed_seq: i64,
) -> Result<(), sqlx::Error> {
	let Some(signer) = &state.capability.webhook_signer else {
		return Ok(());
	};
	let Some(kind) = EventKind::parse(event_kind) else {
		return Ok(());
	};
	let webhooks = state.metadata.active_webhooks().await?;
	if webhooks.is_empty() {
		return Ok(());
	}
	let event = Event {
		protocol: 1,
		event_id: new_id(),
		event_kind: kind,
		project_id: project_id.to_string(),
		game_id: None,
		feed_seq: Some(feed_seq as u64),
		object_digest: Some(object_digest.to_vec()),
		advisory_digest: None,
		issued_at: now(),
	};
	let payload = event.to_canonical_bytes();
	let signature = signer.sign(&event.signing_message());
	let body = serde_json::json!({
		"protocol": 1,
		"event_id": event.event_id,
		"event_kind": event_kind,
		"project_id": project_id,
		"payload": hex::encode(&payload),
		"signature": hex::encode(signature),
	})
	.to_string();
	for webhook in webhooks {
		if !subscribes(&webhook.event_kinds, event_kind) {
			continue;
		}
		state
			.metadata
			.enqueue_delivery(&new_id(), &webhook.id, &event.event_id, &webhook.url, &body, now())
			.await?;
	}
	Ok(())
}

fn subscribes(event_kinds: &str, kind: &str) -> bool {
	event_kinds.is_empty() || event_kinds.split(',').any(|candidate| candidate == kind)
}

pub async fn deliver_pending(state: &AppState, limit: i64) -> Result<usize, String> {
	let due = state
		.metadata
		.due_deliveries(now(), limit)
		.await
		.map_err(|error| error.to_string())?;
	let client = crate::egress::client_builder(&state.capability.tls_extra_roots)
		.timeout(Duration::from_secs(15))
		.redirect(reqwest::redirect::Policy::none())
		.build()
		.map_err(|error| error.to_string())?;
	let mut delivered = 0;
	for delivery in due {
		let allow_local = state.capability.allow_insecure_federation_local;
		let target = match validate_url(&delivery.url, allow_local) {
			Ok(url) => url,
			Err(()) => {
				let _ = state
					.metadata
					.finish_delivery(&delivery.id, delivery.attempt + 1, "failed", now(), None)
					.await;
				continue;
			}
		};
		if crate::egress::guard(&target, allow_local).await.is_err() {
			let _ = state
				.metadata
				.finish_delivery(&delivery.id, delivery.attempt + 1, "failed", now(), None)
				.await;
			continue;
		}
		let result = client
			.post(target)
			.header(reqwest::header::CONTENT_TYPE, "application/json")
			.body(delivery.body)
			.send()
			.await;
		match result {
			Ok(response) if response.status().is_success() => {
				state
					.metadata
					.finish_delivery(&delivery.id, delivery.attempt + 1, "delivered", now(), Some(now()))
					.await
					.map_err(|error| error.to_string())?;
				delivered += 1;
			}
			_ => {
				let attempt = delivery.attempt + 1;
				if attempt >= MAX_ATTEMPTS {
					state
						.metadata
						.finish_delivery(&delivery.id, attempt, "failed", now(), None)
						.await
						.map_err(|error| error.to_string())?;
				} else {
					let backoff = BACKOFF_BASE_SECONDS.saturating_mul(1i64 << attempt.min(10));
					state
						.metadata
						.finish_delivery(&delivery.id, attempt, "pending", now() + backoff, None)
						.await
						.map_err(|error| error.to_string())?;
				}
			}
		}
	}
	Ok(delivered)
}

fn validate_url(value: &str, allow_http_local: bool) -> Result<Url, ()> {
	let url = Url::parse(value).map_err(|_| ())?;
	match url.scheme() {
		"https" => {}
		"http" => {
			let host = url.host_str().unwrap_or_default().to_string();
			let loopback =
				host == "localhost" || host.parse::<std::net::IpAddr>().map(|ip| ip.is_loopback()).unwrap_or(false);
			if !(allow_http_local && loopback) {
				return Err(());
			}
		}
		_ => return Err(()),
	}
	if url.host_str().is_none() {
		return Err(());
	}
	Ok(url)
}

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/webhooks", get(list_webhooks).post(create_webhook))
		.route("/v1/webhooks/{id}", axum::routing::delete(revoke_webhook))
}

#[derive(Serialize)]
struct WebhookView {
	id: String,
	url: String,
	event_kinds: Vec<String>,
	created_at: i64,
}

#[derive(Deserialize)]
struct CreateWebhook {
	url: String,
	#[serde(default)]
	event_kinds: Vec<String>,
}

async fn create_webhook(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<CreateWebhook>,
) -> Response {
	if validate_url(request.url.trim(), state.capability.allow_insecure_federation_local).is_err() {
		return (StatusCode::BAD_REQUEST, "webhook url must be https, or loopback when enabled").into_response();
	}
	if let Some(kind) = request.event_kinds.iter().find(|kind| EventKind::parse(kind).is_none()) {
		return (StatusCode::BAD_REQUEST, format!("unknown event kind `{kind}`")).into_response();
	}
	let id = new_id();
	match state
		.metadata
		.create_webhook(&id, &user.user_id, request.url.trim(), &request.event_kinds.join(","), now())
		.await
	{
		Ok(()) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn list_webhooks(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	match state.metadata.webhooks_for_owner(&user.user_id).await {
		Ok(webhooks) => Json(
			webhooks
				.into_iter()
				.map(|webhook| WebhookView {
					id: webhook.id,
					url: webhook.url,
					event_kinds: webhook
						.event_kinds
						.split(',')
						.filter(|k| !k.is_empty())
						.map(str::to_string)
						.collect(),
					created_at: webhook.created_at,
				})
				.collect::<Vec<_>>(),
		)
		.into_response(),
		Err(error) => storage_error(error),
	}
}

async fn revoke_webhook(State(state): State<AppState>, user: AuthenticatedUser, Path(id): Path<String>) -> Response {
	match state.metadata.revoke_webhook(&user.user_id, &id, now()).await {
		Ok(true) => StatusCode::NO_CONTENT.into_response(),
		Ok(false) => (StatusCode::NOT_FOUND, "no such webhook").into_response(),
		Err(error) => storage_error(error),
	}
}

fn new_id() -> String {
	let mut bytes = [0u8; 16];
	if getrandom::fill(&mut bytes).is_err() {
		panic!("operating system randomness is unavailable");
	}
	hex::encode(bytes)
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "webhook store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}

#[cfg(test)]
mod tests {
	use std::sync::{Arc, Mutex};

	use axum::body::{Body, Bytes, to_bytes};
	use axum::http::header;
	use axum::routing::post as route_post;
	use moraine_crypto::{ALG_ED25519, ObjectKind as Kind, SigningKey, VerifyingKey, object_id};
	use moraine_model::artifact::Artifact;
	use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
	use moraine_model::feed::FeedEntry;
	use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
	use moraine_model::release::ReleasePayload;
	use moraine_model::signed::sign_payload;
	use tokio::net::TcpListener;
	use tower::ServiceExt;

	use super::*;
	use crate::blob::BlobStore;
	use crate::capability::Capability;

	async fn app() -> (Router, AppState, tempfile::TempDir) {
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
			tls_extra_roots: None,
			max_feed_scan_pages: 50,
			skip_migrate_on_start: false,
			database_url: None,
			allow_insecure_federation_local: true,
			publishing: crate::config::Publishing::Open,
			web_dir: None,
		};
		let state = AppState {
			store,
			metadata,
			capability: Arc::new(Capability::discover(&config)),
			login_limiter: std::sync::Arc::new(crate::auth::LoginLimiter::new()),
			metrics: std::sync::Arc::new(crate::metrics::Metrics::new()),
			rate_limiter: std::sync::Arc::new(crate::ratelimit::RateLimiter::new()),
			web_dir: None,
		};
		(crate::routes::router(state.clone()), state, directory)
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

	fn genesis_wire(signer: &SigningKey) -> Vec<u8> {
		let genesis = Genesis {
			protocol: 1,
			kind: GenesisKind::Project,
			nonce: vec![0x11; 16],
			roots: vec![RootKey::from_public_key(signer.verifying_key().to_bytes().to_vec()).expect("root")],
			threshold: 1,
			authorized_kinds: vec!["delegation".to_string(), "release".to_string(), "profile".to_string()],
			home_hint: None,
			contacts: None,
			created_at: 1_760_000_000,
		};
		sign_payload(Kind::Genesis, &genesis, &[signer]).wire_bytes()
	}

	fn release_wire(signer: &SigningKey, project_id: &str) -> ([u8; 32], Vec<u8>) {
		let release = ReleasePayload {
			protocol: 1,
			project_id: project_id.to_string(),
			game_id: "gd:sha256:game".to_string(),
			release_nonce: vec![0x42; 16],
			human_version: "1.0.0".to_string(),
			channel: "release".to_string(),
			kind: "mod".to_string(),
			declared_time: 1_760_000_000,
			compatibility: vec![Compatibility {
				game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
				loader_id: None,
				loader_version_predicate: None,
				side: Side::Both,
				runtime_predicate: None,
				os_predicate: None,
				arch_predicate: None,
			}],
			artifacts: vec![Artifact {
				digest: vec![0xAB; 32],
				size: 10,
				media_type: "application/java-archive".to_string(),
				filename: "example.jar".to_string(),
				is_primary: true,
				os_predicate: None,
				arch_predicate: None,
			}],
			dependencies: Vec::new(),
			source_reference: None,
			changelog_digest: None,
			license_expression: None,
			rights: None,
			sbom_digest: None,
			minimum_verifier_version: 1,
			critical_extensions: Vec::new(),
		};
		let signed = sign_payload(Kind::Release, &release, &[signer]);
		(object_id(Kind::Release, &signed.payload_bytes), signed.wire_bytes())
	}

	fn feed_wire(signer: &SigningKey, project_id: &str, sequence: u64, object: [u8; 32]) -> Vec<u8> {
		let entry = FeedEntry {
			protocol: 1,
			project_id: project_id.to_string(),
			sequence,
			previous: None,
			kind: "release-published".to_string(),
			object_digest: object.to_vec(),
			declared_at: 1_760_000_000 + sequence as i64,
		};
		sign_payload(Kind::FeedEntry, &entry, &[signer]).wire_bytes()
	}

	#[tokio::test]
	async fn delivers_a_signed_event_and_verifies_it() {
		let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
		let sink = {
			let received = received.clone();
			Router::new().route(
				"/sink",
				route_post(move |body: Bytes| {
					let received = received.clone();
					async move {
						let value: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
						received.lock().expect("lock").push(value);
						StatusCode::NO_CONTENT
					}
				}),
			)
		};
		let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
		let address = listener.local_addr().expect("addr");
		tokio::spawn(async move {
			let _ = axum::serve(listener, sink).await;
		});

		let (application, state, _directory) = app().await;
		let signer = SigningKey::from_seed(&[41u8; 32]);
		let (cookie, csrf) = login(&application, "operator@example.org").await;

		let create = axum::http::Request::builder()
			.method("POST")
			.uri("/v1/webhooks")
			.header(header::CONTENT_TYPE, "application/json")
			.header(header::COOKIE, &cookie)
			.header("x-csrf-token", &csrf)
			.body(Body::from(
				serde_json::json!({
					"url": format!("http://127.0.0.1:{}/sink", address.port()),
					"event_kinds": ["release-published"],
				})
				.to_string(),
			))
			.expect("request");
		let response = application.clone().oneshot(create).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let request = axum::http::Request::post("/v1/projects")
			.body(Body::from(genesis_wire(&signer)))
			.expect("request");
		let response = application.clone().oneshot(request).await.expect("response");
		let project_id = body_json(response).await["project_id"]
			.as_str()
			.expect("project id")
			.to_string();

		let (release_digest, release) = release_wire(&signer, &project_id);
		let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
			.body(Body::from(release))
			.expect("request");
		application.clone().oneshot(request).await.expect("response");

		let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
			.body(Body::from(feed_wire(&signer, &project_id, 1, release_digest)))
			.expect("request");
		let response = application.oneshot(request).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let delivered = deliver_pending(&state, 10).await.expect("deliver");
		assert_eq!(delivered, 1);

		let bodies = received.lock().expect("lock").clone();
		assert_eq!(bodies.len(), 1);
		let payload = hex::decode(bodies[0]["payload"].as_str().expect("payload")).expect("hex");
		let signature = hex::decode(bodies[0]["signature"].as_str().expect("signature")).expect("hex");
		let public_key = state.capability.webhook_public_key.as_ref().expect("public key");
		let key = VerifyingKey::from_bytes(ALG_ED25519, &hex::decode(public_key).expect("hex")).expect("key");
		let mut message = moraine_model::event::WEBHOOK_DOMAIN.to_vec();
		message.extend_from_slice(&payload);
		assert!(key.verify(&message, &signature).is_ok());

		let again = deliver_pending(&state, 10).await.expect("deliver");
		assert_eq!(again, 0);
	}
}

#[cfg(test)]
mod prune_tests {
	use super::*;

	#[tokio::test]
	async fn prunes_delivery_records_past_retention() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		store
			.enqueue_delivery("old", "wh", "e1", "https://mirror.example", "{}", 1_000)
			.await
			.expect("old");
		store
			.enqueue_delivery("fresh", "wh", "e2", "https://mirror.example", "{}", now())
			.await
			.expect("fresh");

		let pruned = store.prune_deliveries(now() - 30 * 86_400).await.expect("prune");
		assert_eq!(pruned, 1);
	}
}
