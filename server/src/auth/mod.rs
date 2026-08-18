pub mod accounts;
pub mod orgs;
pub mod password;
pub mod ratelimit;
pub mod teams;

use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::auth::accounts::{ApiKeyRow, SessionRow};
use crate::routes::AppState;

const SESSION_COOKIE: &str = "moraine_session";
const CSRF_COOKIE: &str = "moraine_csrf";
const IDLE_SECONDS: i64 = 30 * 24 * 3600;
const ABSOLUTE_SECONDS: i64 = 90 * 24 * 3600;
const API_KEY_DEFAULT_EXPIRY: i64 = 90 * 24 * 3600;
const MIN_PASSWORD_LENGTH: usize = 12;
const LOGIN_MAX_ATTEMPTS: u32 = 10;
const LOGIN_WINDOW_SECONDS: i64 = 15 * 60;

pub struct LoginLimiter {
	attempts: std::sync::Mutex<std::collections::HashMap<String, (u32, i64)>>,
}

impl LoginLimiter {
	pub fn new() -> Self {
		Self {
			attempts: std::sync::Mutex::new(std::collections::HashMap::new()),
		}
	}

	fn check(&self, key: &str, now: i64) -> Result<(), i64> {
		let attempts = self.attempts.lock().expect("login limiter");
		if let Some((count, start)) = attempts.get(key)
			&& now - start < LOGIN_WINDOW_SECONDS
			&& *count >= LOGIN_MAX_ATTEMPTS
		{
			return Err(start + LOGIN_WINDOW_SECONDS - now);
		}
		Ok(())
	}

	fn record_failure(&self, key: &str, now: i64) {
		let mut attempts = self.attempts.lock().expect("login limiter");
		let entry = attempts.entry(key.to_string()).or_insert((0, now));
		if now - entry.1 >= LOGIN_WINDOW_SECONDS {
			*entry = (0, now);
		}
		entry.0 += 1;
	}

	fn clear(&self, key: &str) {
		self.attempts.lock().expect("login limiter").remove(key);
	}
}

impl Default for LoginLimiter {
	fn default() -> Self {
		Self::new()
	}
}

pub const KNOWN_SCOPES: &[&str] = &[
	"account:read",
	"keys:manage",
	"projects:write",
	"artifacts:write",
	"submissions:write",
	"submissions:review",
	"federation:manage",
	"orgs:manage",
	"notifications:read",
];

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/auth/register", post(register))
		.route("/v1/auth/session", post(login).delete(logout))
		.route("/v1/auth/me", get(me))
		.route("/v1/auth/keys", get(list_keys).post(create_key))
		.route("/v1/auth/keys/{id}", delete(revoke_key))
}

pub struct AuthenticatedUser {
	pub user_id: String,
	pub scopes: Vec<String>,
	pub session_id: Option<String>,
	pub api_key_id: Option<String>,
}

impl AuthenticatedUser {
	pub fn allows(&self, scope: &str) -> bool {
		self.session_id.is_some() || self.scopes.iter().any(|granted| granted == scope)
	}

	pub fn via(&self) -> &'static str {
		if self.api_key_id.is_some() { "api-key" } else { "session" }
	}
}

impl FromRequestParts<AppState> for AuthenticatedUser {
	type Rejection = Response;

	async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
		let now = now();
		if let Some(token) = bearer_token(parts) {
			let key = state
				.metadata
				.api_key_by_token(&token_hash(&token), now)
				.await
				.map_err(storage_error)?;
			let Some(key) = key else {
				return Err(unauthorized());
			};
			let _ = state.metadata.touch_api_key(&key.id, now).await;
			return Ok(Self {
				user_id: key.user_id,
				scopes: split_scopes(&key.scopes),
				session_id: None,
				api_key_id: Some(key.id),
			});
		}
		if let Some(token) = cookie(parts.headers.get(header::COOKIE), SESSION_COOKIE) {
			let session = state
				.metadata
				.session_by_token(&token_hash(&token), now)
				.await
				.map_err(storage_error)?;
			let Some(session) = session else {
				return Err(unauthorized());
			};
			if is_mutating(&parts.method) {
				let header_token = parts.headers.get("x-csrf-token").and_then(|value| value.to_str().ok());
				let cookie_token = cookie(parts.headers.get(header::COOKIE), CSRF_COOKIE);
				if header_token.is_none() || header_token != cookie_token.as_deref() {
					return Err((StatusCode::FORBIDDEN, "missing or mismatched CSRF token").into_response());
				}
			}
			let _ = state.metadata.touch_session(&session.id, now, now + IDLE_SECONDS).await;
			return Ok(Self {
				user_id: session.user_id,
				scopes: Vec::new(),
				session_id: Some(session.id),
				api_key_id: None,
			});
		}
		Err(unauthorized())
	}
}

#[derive(Deserialize)]
struct Credentials {
	email: String,
	password: String,
}

#[derive(Serialize)]
struct RegisterReceipt {
	user_id: String,
}

#[derive(Serialize)]
struct LoginReceipt {
	user_id: String,
	expires_at: i64,
}

async fn register(State(state): State<AppState>, Json(credentials): Json<Credentials>) -> Response {
	let email = credentials.email.trim().to_lowercase();
	if !email.contains('@') {
		return (StatusCode::BAD_REQUEST, "invalid email").into_response();
	}
	if credentials.password.len() < MIN_PASSWORD_LENGTH {
		return (StatusCode::BAD_REQUEST, "password is too short").into_response();
	}
	match state.metadata.user_by_email(&email).await {
		Ok(Some(_)) => return (StatusCode::CONFLICT, "email already registered").into_response(),
		Ok(None) => {}
		Err(error) => return storage_error(error),
	}
	let Ok(password_hash) = password::hash_password(&credentials.password) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "password hashing failed").into_response();
	};
	let user_id = new_id();
	if let Err(error) = state.metadata.create_user(&user_id, &email, &password_hash, now()).await {
		return storage_error(error);
	}
	(StatusCode::CREATED, Json(RegisterReceipt { user_id })).into_response()
}

async fn login(State(state): State<AppState>, Json(credentials): Json<Credentials>) -> Response {
	let email = credentials.email.trim().to_lowercase();
	if let Err(retry_after) = state.login_limiter.check(&email, now()) {
		let mut response = (StatusCode::TOO_MANY_REQUESTS, "too many login attempts").into_response();
		response.headers_mut().insert(
			header::RETRY_AFTER,
			retry_after.max(1).to_string().parse().expect("valid header"),
		);
		return response;
	}
	let user = match state.metadata.user_by_email(&email).await {
		Ok(Some(user)) => user,
		Ok(None) => return (StatusCode::UNAUTHORIZED, "invalid credentials").into_response(),
		Err(error) => return storage_error(error),
	};
	if !password::verify_password(&credentials.password, &user.password_hash) {
		state.login_limiter.record_failure(&email, now());
		return (StatusCode::UNAUTHORIZED, "invalid credentials").into_response();
	}
	state.login_limiter.clear(&email);
	let token = random_token();
	let csrf = random_token();
	let current = now();
	let session = SessionRow {
		id: new_id(),
		user_id: user.id.clone(),
		token_hash: token_hash(&token).to_vec(),
		created_at: current,
		idle_expires_at: current + IDLE_SECONDS,
		absolute_expires_at: current + ABSOLUTE_SECONDS,
	};
	if let Err(error) = state.metadata.create_session(&session).await {
		return storage_error(error);
	}
	let mut response = Json(LoginReceipt {
		user_id: user.id,
		expires_at: session.absolute_expires_at,
	})
	.into_response();
	append_cookie(&mut response, &session_cookie(&token));
	append_cookie(&mut response, &csrf_cookie(&csrf));
	response
}

async fn logout(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if let Some(session_id) = &user.session_id
		&& let Err(error) = state.metadata.revoke_session(session_id, now()).await
	{
		return storage_error(error);
	}
	let mut response = StatusCode::NO_CONTENT.into_response();
	append_cookie(&mut response, &clear_cookie(SESSION_COOKIE, true));
	append_cookie(&mut response, &clear_cookie(CSRF_COOKIE, false));
	response
}

#[derive(Serialize)]
struct AccountView {
	user_id: String,
	email: String,
	via: &'static str,
}

async fn me(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	match state.metadata.user_by_id(&user.user_id).await {
		Ok(Some(record)) => Json(AccountView {
			user_id: record.id,
			email: record.email,
			via: user.via(),
		})
		.into_response(),
		Ok(None) => (StatusCode::UNAUTHORIZED, "account no longer exists").into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Serialize)]
struct ApiKeyView {
	id: String,
	name: String,
	prefix: String,
	scopes: Vec<String>,
	created_at: i64,
	expires_at: Option<i64>,
	last_used_at: Option<i64>,
}

async fn list_keys(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if !user.allows("keys:manage") {
		return forbidden();
	}
	match state.metadata.api_keys_for_user(&user.user_id).await {
		Ok(keys) => {
			let view: Vec<ApiKeyView> = keys
				.into_iter()
				.map(|key| ApiKeyView {
					id: key.id,
					name: key.name,
					prefix: key.prefix,
					scopes: split_scopes(&key.scopes),
					created_at: key.created_at,
					expires_at: key.expires_at,
					last_used_at: key.last_used_at,
				})
				.collect();
			Json(view).into_response()
		}
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
struct CreateKey {
	name: String,
	#[serde(default)]
	scopes: Vec<String>,
	#[serde(default)]
	expires_in_days: Option<i64>,
}

#[derive(Serialize)]
struct CreatedKey {
	id: String,
	name: String,
	prefix: String,
	key: String,
	scopes: Vec<String>,
	expires_at: i64,
}

async fn create_key(State(state): State<AppState>, user: AuthenticatedUser, Json(request): Json<CreateKey>) -> Response {
	if !user.allows("keys:manage") {
		return forbidden();
	}
	if request.name.trim().is_empty() {
		return (StatusCode::BAD_REQUEST, "key name is required").into_response();
	}
	if let Some(scope) = request.scopes.iter().find(|scope| !KNOWN_SCOPES.contains(&scope.as_str())) {
		return (StatusCode::BAD_REQUEST, format!("unknown scope `{scope}`")).into_response();
	}
	let secret = format!("mrn_{}", random_token());
	let prefix = secret[..12].to_string();
	let current = now();
	let expires_at = current
		+ request
			.expires_in_days
			.map_or(API_KEY_DEFAULT_EXPIRY, |days| days.clamp(1, 365) * 86_400);
	let key = ApiKeyRow {
		id: new_id(),
		user_id: user.user_id,
		name: request.name,
		prefix: prefix.clone(),
		secret_hash: token_hash(&secret).to_vec(),
		scopes: request.scopes.join(","),
		created_at: current,
		expires_at: Some(expires_at),
		last_used_at: None,
	};
	if let Err(error) = state.metadata.create_api_key(&key).await {
		return storage_error(error);
	}
	let response = CreatedKey {
		id: key.id,
		name: key.name,
		prefix,
		key: secret,
		scopes: request.scopes,
		expires_at,
	};
	(StatusCode::CREATED, Json(response)).into_response()
}

async fn revoke_key(State(state): State<AppState>, user: AuthenticatedUser, Path(id): Path<String>) -> Response {
	if !user.allows("keys:manage") {
		return forbidden();
	}
	match state.metadata.revoke_api_key(&user.user_id, &id, now()).await {
		Ok(true) => StatusCode::NO_CONTENT.into_response(),
		Ok(false) => (StatusCode::NOT_FOUND, "no such key").into_response(),
		Err(error) => storage_error(error),
	}
}

fn bearer_token(parts: &Parts) -> Option<String> {
	let value = parts.headers.get(header::AUTHORIZATION)?.to_str().ok()?;
	value.strip_prefix("Bearer ").map(str::to_string)
}

fn cookie(header: Option<&axum::http::HeaderValue>, name: &str) -> Option<String> {
	let header = header?.to_str().ok()?;
	header
		.split(';')
		.filter_map(|pair| pair.trim().split_once('='))
		.find(|(key, _)| *key == name)
		.map(|(_, value)| value.to_string())
}

fn is_mutating(method: &Method) -> bool {
	matches!(*method, Method::POST | Method::PUT | Method::PATCH | Method::DELETE)
}

fn append_cookie(response: &mut Response, cookie: &str) {
	if let Ok(value) = header::HeaderValue::from_str(cookie) {
		response.headers_mut().append(header::SET_COOKIE, value);
	}
}

fn session_cookie(token: &str) -> String {
	format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={ABSOLUTE_SECONDS}")
}

fn csrf_cookie(token: &str) -> String {
	format!("{CSRF_COOKIE}={token}; Path=/; Secure; SameSite=Lax; Max-Age={ABSOLUTE_SECONDS}")
}

fn clear_cookie(name: &str, http_only: bool) -> String {
	let mut cookie = format!("{name}=; Path=/; Secure; SameSite=Lax; Max-Age=0");
	if http_only {
		cookie.push_str("; HttpOnly");
	}
	cookie
}

fn split_scopes(scopes: &str) -> Vec<String> {
	if scopes.is_empty() {
		Vec::new()
	} else {
		scopes.split(',').map(str::to_string).collect()
	}
}

fn token_hash(token: &str) -> [u8; 32] {
	Sha256::digest(token.as_bytes()).into()
}

fn random_token() -> String {
	hex::encode(new_id_bytes())
}

fn new_id() -> String {
	hex::encode(new_id_bytes())
}

fn new_id_bytes() -> [u8; 16] {
	let mut bytes = [0u8; 16];
	if getrandom::fill(&mut bytes).is_err() {
		panic!("operating system randomness is unavailable");
	}
	bytes
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

fn unauthorized() -> Response {
	(StatusCode::UNAUTHORIZED, "authentication required").into_response()
}

fn forbidden() -> Response {
	(StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response()
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "account store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}

#[cfg(test)]
mod tests {

	use std::sync::Arc;

	use axum::body::{Body, to_bytes};
	use tower::ServiceExt;

	use super::*;
	use crate::blob::BlobStore;
	use crate::capability::Capability;
	use crate::db::MetadataStore;

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
			publishing: crate::config::Publishing::Review,
			web_dir: None,
		};
		let state = AppState {
			store,
			metadata,
			capability: Arc::new(Capability::discover(&config)),
			login_limiter: Arc::new(crate::auth::LoginLimiter::new()),
			metrics: Arc::new(crate::ops::metrics::Metrics::new()),
			rate_limiter: Arc::new(crate::auth::ratelimit::RateLimiter::new()),
			web_dir: None,
		};
		(crate::routes::router(state), directory)
	}

	fn json_request(method: &str, path: &str, body: serde_json::Value) -> axum::http::Request<Body> {
		axum::http::Request::builder()
			.method(method)
			.uri(path)
			.header(header::CONTENT_TYPE, "application/json")
			.body(Body::from(body.to_string()))
			.expect("request")
	}

	fn cookie_token(response: &Response, name: &str) -> Option<String> {
		response.headers().get_all(header::SET_COOKIE).iter().find_map(|value| {
			let cookie = value.to_str().ok()?;
			let first = cookie.split(';').next()?;
			first.strip_prefix(&format!("{name}=")).map(str::to_string)
		})
	}

	async fn json_body(response: Response) -> serde_json::Value {
		let bytes = to_bytes(response.into_body(), 64 * 1024).await.expect("body");
		serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
	}

	#[tokio::test]
	async fn registers_logs_in_and_lists_a_key() {
		let (application, _directory) = app().await;
		let register = json_request(
			"POST",
			"/v1/auth/register",
			serde_json::json!({ "email": "author@example.org", "password": "correct horse battery" }),
		);
		let response = application.clone().oneshot(register).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);
		let body = json_body(response).await;
		assert!(body["user_id"].is_string());

		let duplicate = json_request(
			"POST",
			"/v1/auth/register",
			serde_json::json!({ "email": "author@example.org", "password": "correct horse battery" }),
		);
		let response = application.clone().oneshot(duplicate).await.expect("response");
		assert_eq!(response.status(), StatusCode::CONFLICT);

		let login = json_request(
			"POST",
			"/v1/auth/session",
			serde_json::json!({ "email": "author@example.org", "password": "correct horse battery" }),
		);
		let response = application.clone().oneshot(login).await.expect("response");
		assert_eq!(response.status(), StatusCode::OK);
		let session = cookie_token(&response, SESSION_COOKIE).expect("session cookie");
		let csrf = cookie_token(&response, CSRF_COOKIE).expect("csrf cookie");

		let me = axum::http::Request::builder()
			.method("GET")
			.uri("/v1/auth/me")
			.header(header::COOKIE, format!("{SESSION_COOKIE}={session}"))
			.body(Body::empty())
			.expect("request");
		let response = application.clone().oneshot(me).await.expect("response");
		assert_eq!(response.status(), StatusCode::OK);
		let body = json_body(response).await;
		assert_eq!(body["via"], "session");

		let create = axum::http::Request::builder()
			.method("POST")
			.uri("/v1/auth/keys")
			.header(header::CONTENT_TYPE, "application/json")
			.header(header::COOKIE, format!("{SESSION_COOKIE}={session}; {CSRF_COOKIE}={csrf}"))
			.header("x-csrf-token", csrf.clone())
			.body(Body::from(
				serde_json::json!({ "name": "ci", "scopes": ["keys:manage"] }).to_string(),
			))
			.expect("request");
		let response = application.clone().oneshot(create).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);
		let body = json_body(response).await;
		let secret = body["key"].as_str().expect("key").to_string();

		let me_via_key = axum::http::Request::builder()
			.method("GET")
			.uri("/v1/auth/me")
			.header(header::AUTHORIZATION, format!("Bearer {secret}"))
			.body(Body::empty())
			.expect("request");
		let response = application.clone().oneshot(me_via_key).await.expect("response");
		assert_eq!(response.status(), StatusCode::OK);
		let body = json_body(response).await;
		assert_eq!(body["via"], "api-key");
	}

	#[tokio::test]
	async fn rejects_cookie_write_without_csrf_and_requires_authentication() {
		let (application, _directory) = app().await;
		let me = axum::http::Request::get("/v1/auth/me").body(Body::empty()).expect("request");
		let response = application.clone().oneshot(me).await.expect("response");
		assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

		let register = json_request(
			"POST",
			"/v1/auth/register",
			serde_json::json!({ "email": "a@b.org", "password": "correct horse battery" }),
		);
		application.clone().oneshot(register).await.expect("response");
		let login = json_request(
			"POST",
			"/v1/auth/session",
			serde_json::json!({ "email": "a@b.org", "password": "correct horse battery" }),
		);
		let response = application.clone().oneshot(login).await.expect("response");
		let session = cookie_token(&response, SESSION_COOKIE).expect("session cookie");

		let create = axum::http::Request::builder()
			.method("POST")
			.uri("/v1/auth/keys")
			.header(header::CONTENT_TYPE, "application/json")
			.header(header::COOKIE, format!("{SESSION_COOKIE}={session}"))
			.body(Body::from(serde_json::json!({ "name": "ci" }).to_string()))
			.expect("request");
		let response = application.oneshot(create).await.expect("response");
		assert_eq!(response.status(), StatusCode::FORBIDDEN);
	}

	#[tokio::test]
	async fn login_is_rate_limited_per_account() {
		let (application, _directory) = app().await;
		let register = json_request(
			"POST",
			"/v1/auth/register",
			serde_json::json!({ "email": "limit@example.org", "password": "correct horse battery" }),
		);
		application.clone().oneshot(register).await.expect("response");

		for _ in 0..10 {
			let wrong = json_request(
				"POST",
				"/v1/auth/session",
				serde_json::json!({ "email": "limit@example.org", "password": "wrong password here" }),
			);
			let response = application.clone().oneshot(wrong).await.expect("response");
			assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
		}

		let correct = json_request(
			"POST",
			"/v1/auth/session",
			serde_json::json!({ "email": "limit@example.org", "password": "correct horse battery" }),
		);
		let response = application.oneshot(correct).await.expect("response");
		assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
		assert!(response.headers().get(header::RETRY_AFTER).is_some());
	}
}
