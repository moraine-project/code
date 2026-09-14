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
const RECOVERY_CODE_COUNT: usize = 8;
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
		if attempts.len() > 10_000 {
			attempts.retain(|_, (_, start)| now - *start < LOGIN_WINDOW_SECONDS);
		}
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
	"directory:manage",
	"notifications:read",
];

pub const OPERATOR_ROLE: &str = "operator";

pub const SESSION_SCOPES: &[&str] = &[
	"account:read",
	"keys:manage",
	"projects:write",
	"artifacts:write",
	"submissions:write",
	"notifications:read",
];

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/auth/register", post(register))
		.route("/v1/auth/session", post(login).delete(logout))
		.route("/v1/auth/me", get(me))
		.route("/v1/auth/keys", get(list_keys).post(create_key))
		.route("/v1/auth/keys/{id}", delete(revoke_key))
		.route("/v1/auth/password", post(change_password))
		.route("/v1/auth/recovery-codes", post(issue_recovery_codes))
		.route("/v1/auth/recover", post(recover))
		.route("/v1/auth/users/reset-password", post(reset_password))
}

pub struct AuthenticatedUser {
	pub user_id: String,
	pub scopes: Vec<String>,
	pub session_id: Option<String>,
	pub api_key_id: Option<String>,
	pub is_operator: bool,
}

impl AuthenticatedUser {
	pub fn allows(&self, scope: &str) -> bool {
		if self.session_id.is_some() {
			return self.is_operator || SESSION_SCOPES.contains(&scope);
		}
		self.scopes.iter().any(|granted| granted == scope)
	}

	pub fn mintable_scopes(&self) -> Vec<&str> {
		if self.session_id.is_some() && self.is_operator {
			return KNOWN_SCOPES.to_vec();
		}
		if self.session_id.is_some() {
			return SESSION_SCOPES.to_vec();
		}
		self.scopes.iter().map(String::as_str).collect()
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
				is_operator: false,
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
				if !origin_trusted(parts, state) {
					return Err((StatusCode::FORBIDDEN, "request origin is not allowed").into_response());
				}
			}
			let _ = state.metadata.touch_session(&session.id, now, now + IDLE_SECONDS).await;
			let role = state.metadata.user_role(&session.user_id).await.map_err(storage_error)?;
			let is_operator = role.as_deref() == Some(OPERATOR_ROLE);
			return Ok(Self {
				user_id: session.user_id,
				scopes: Vec::new(),
				session_id: Some(session.id),
				api_key_id: None,
				is_operator,
			});
		}
		Err(unauthorized())
	}
}

pub struct ClientIp(pub Option<std::net::IpAddr>);

impl FromRequestParts<AppState> for ClientIp {
	type Rejection = std::convert::Infallible;

	async fn from_request_parts(parts: &mut Parts, _state: &AppState) -> Result<Self, Self::Rejection> {
		let ip = parts
			.extensions
			.get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
			.map(|axum::extract::ConnectInfo(address)| address.ip());
		Ok(Self(ip))
	}
}

fn dummy_hash() -> &'static str {
	static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
	HASH.get_or_init(|| password::hash_password("not a real password").unwrap_or_default())
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
	csrf: String,
}

async fn register(State(state): State<AppState>, Json(credentials): Json<Credentials>) -> Response {
	if !state.capability.registration_open {
		return (StatusCode::FORBIDDEN, "this instance is not accepting new accounts").into_response();
	}
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
	if let Err(error) = state
		.metadata
		.create_user(&user_id, &email, &password_hash, "member", now())
		.await
	{
		return storage_error(error);
	}
	(StatusCode::CREATED, Json(RegisterReceipt { user_id })).into_response()
}

async fn login(State(state): State<AppState>, client: ClientIp, Json(credentials): Json<Credentials>) -> Response {
	let email = credentials.email.trim().to_lowercase();
	let key = format!(
		"{}|{}",
		email,
		client.0.map(|address| address.to_string()).unwrap_or_default()
	);
	if let Err(retry_after) = state.login_limiter.check(&key, now()) {
		let mut response = (StatusCode::TOO_MANY_REQUESTS, "too many login attempts").into_response();
		response.headers_mut().insert(
			header::RETRY_AFTER,
			retry_after.max(1).to_string().parse().expect("valid header"),
		);
		return response;
	}
	let user = match state.metadata.user_by_email(&email).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			let _ = password::verify_password(&credentials.password, dummy_hash());
			return (StatusCode::UNAUTHORIZED, "invalid credentials").into_response();
		}
		Err(error) => return storage_error(error),
	};
	if !password::verify_password(&credentials.password, &user.password_hash) {
		state.login_limiter.record_failure(&key, now());
		return (StatusCode::UNAUTHORIZED, "invalid credentials").into_response();
	}
	state.login_limiter.clear(&key);
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
	state.metrics.record_session_created();
	let cross_origin = state.capability.cross_origin();
	let mut response = Json(LoginReceipt {
		user_id: user.id,
		expires_at: session.absolute_expires_at,
		csrf: csrf.clone(),
	})
	.into_response();
	append_cookie(&mut response, &session_cookie(&token, cross_origin));
	append_cookie(&mut response, &csrf_cookie(&csrf, cross_origin));
	response
}

async fn logout(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if let Some(session_id) = &user.session_id {
		if let Err(error) = state.metadata.revoke_session(session_id, now()).await {
			return storage_error(error);
		}
		state.metrics.record_session_revoked();
	}
	let cross_origin = state.capability.cross_origin();
	let mut response = StatusCode::NO_CONTENT.into_response();
	append_cookie(&mut response, &clear_cookie(SESSION_COOKIE, true, cross_origin));
	append_cookie(&mut response, &clear_cookie(CSRF_COOKIE, false, cross_origin));
	response
}

#[derive(Deserialize)]
struct PasswordChange {
	current: String,
	new: String,
}

async fn change_password(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<PasswordChange>,
) -> Response {
	let Some(session_id) = user.session_id.clone() else {
		return (StatusCode::FORBIDDEN, "change a password from a signed-in session").into_response();
	};
	if request.new.len() < MIN_PASSWORD_LENGTH {
		return (StatusCode::BAD_REQUEST, "the new password is too short").into_response();
	}
	let record = match state.metadata.user_by_id(&user.user_id).await {
		Ok(Some(record)) => record,
		Ok(None) => return (StatusCode::UNAUTHORIZED, "account no longer exists").into_response(),
		Err(error) => return storage_error(error),
	};
	if !password::verify_password(&request.current, &record.password_hash) {
		return (StatusCode::UNAUTHORIZED, "the current password is wrong").into_response();
	}
	let Ok(hash) = password::hash_password(&request.new) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "password hashing failed").into_response();
	};
	if let Err(error) = state.metadata.update_password(&user.user_id, &hash, now()).await {
		return storage_error(error);
	}
	if let Err(error) = state.metadata.revoke_other_sessions(&user.user_id, &session_id, now()).await {
		return storage_error(error);
	}
	StatusCode::NO_CONTENT.into_response()
}

async fn issue_recovery_codes(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if user.session_id.is_none() {
		return (StatusCode::FORBIDDEN, "issue recovery codes from a signed-in session").into_response();
	}
	let codes: Vec<String> = (0..RECOVERY_CODE_COUNT).map(|_| random_token()).collect();
	let hashes: Vec<Vec<u8>> = codes.iter().map(|code| token_hash(code).to_vec()).collect();
	if let Err(error) = state.metadata.replace_recovery_codes(&user.user_id, &hashes, now()).await {
		return storage_error(error);
	}
	Json(serde_json::json!({ "codes": codes })).into_response()
}

#[derive(Deserialize)]
struct RecoverRequest {
	email: String,
	code: String,
	new: String,
}

async fn recover(State(state): State<AppState>, Json(request): Json<RecoverRequest>) -> Response {
	let email = request.email.trim().to_lowercase();
	if request.new.len() < MIN_PASSWORD_LENGTH {
		return (StatusCode::BAD_REQUEST, "the new password is too short").into_response();
	}
	let Some(record) = (match state.metadata.user_by_email(&email).await {
		Ok(record) => record,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::UNAUTHORIZED, "invalid recovery code").into_response();
	};
	let hash = token_hash(request.code.trim());
	let valid = match state.metadata.has_unused_recovery_code(&record.id, &hash).await {
		Ok(valid) => valid,
		Err(error) => return storage_error(error),
	};
	if !valid {
		return (StatusCode::UNAUTHORIZED, "invalid recovery code").into_response();
	}
	let Ok(password_hash) = password::hash_password(&request.new) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "password hashing failed").into_response();
	};
	if let Err(error) = state.metadata.update_password(&record.id, &password_hash, now()).await {
		return storage_error(error);
	}
	let _ = state.metadata.consume_recovery_code(&record.id, &hash, now()).await;
	if let Err(error) = state.metadata.revoke_sessions(&record.id, now()).await {
		return storage_error(error);
	}
	StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct ResetPasswordRequest {
	email: String,
}

async fn reset_password(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<ResetPasswordRequest>,
) -> Response {
	if !user.allows("directory:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let email = request.email.trim().to_lowercase();
	let Some(record) = (match state.metadata.user_by_email(&email).await {
		Ok(record) => record,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::NOT_FOUND, "no account with that email").into_response();
	};
	let temporary = random_token();
	let Ok(password_hash) = password::hash_password(&temporary) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "password hashing failed").into_response();
	};
	if let Err(error) = state.metadata.update_password(&record.id, &password_hash, now()).await {
		return storage_error(error);
	}
	if let Err(error) = state.metadata.revoke_sessions(&record.id, now()).await {
		return storage_error(error);
	}
	Json(serde_json::json!({ "password": temporary })).into_response()
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
	let mintable = user.mintable_scopes();
	if let Some(scope) = request.scopes.iter().find(|scope| !mintable.contains(&scope.as_str())) {
		return (StatusCode::FORBIDDEN, format!("you cannot grant `{scope}`")).into_response();
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
	state.metrics.record_api_key_created();
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
		Ok(true) => {
			state.metrics.record_api_key_revoked();
			StatusCode::NO_CONTENT.into_response()
		}
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

fn same_site(cross_origin: bool) -> &'static str {
	if cross_origin { "None" } else { "Lax" }
}

fn session_cookie(token: &str, cross_origin: bool) -> String {
	format!(
		"{SESSION_COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite={}; Max-Age={ABSOLUTE_SECONDS}",
		same_site(cross_origin)
	)
}

fn csrf_cookie(token: &str, cross_origin: bool) -> String {
	format!(
		"{CSRF_COOKIE}={token}; Path=/; Secure; SameSite={}; Max-Age={ABSOLUTE_SECONDS}",
		same_site(cross_origin)
	)
}

fn clear_cookie(name: &str, http_only: bool, cross_origin: bool) -> String {
	let mut cookie = format!("{name}=; Path=/; Secure; SameSite={}; Max-Age=0", same_site(cross_origin));
	if http_only {
		cookie.push_str("; HttpOnly");
	}
	cookie
}

fn origin_trusted(parts: &Parts, state: &AppState) -> bool {
	let origin = parts
		.headers
		.get(header::ORIGIN)
		.and_then(|value| value.to_str().ok())
		.or_else(|| parts.headers.get(header::REFERER).and_then(|value| value.to_str().ok()));
	let Some(origin) = origin else {
		return true;
	};
	if state.capability.origin_allowed(origin) {
		return true;
	}
	let Some(host) = parts.headers.get(header::HOST).and_then(|value| value.to_str().ok()) else {
		return true;
	};
	origin_host(origin) == Some(host)
}

fn origin_host(origin: &str) -> Option<&str> {
	let rest = origin.split_once("://").map_or(origin, |(_, rest)| rest);
	rest.split('/').next()
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

pub(crate) fn now() -> i64 {
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
mod tests;
