pub mod accounts;
pub mod orgs;
pub mod password;
pub mod ratelimit;
pub mod teams;

use axum::Router;
use axum::extract::{Json, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use serde::Deserialize;

use crate::routes::AppState;

mod admin;
mod api_keys;
mod cookies;
mod credentials;
mod recovery;
mod security;
mod session;
mod verification;

pub(crate) use security::now;
pub(super) use security::{new_id, random_token, storage_error, token_hash};
pub(crate) use session::AuthenticatedUser;
pub(crate) use verification::verified_or_error;

pub(crate) const SESSION_COOKIE: &str = "moraine_session";
pub(crate) const CSRF_COOKIE: &str = "moraine_csrf";
const IDLE_SECONDS: i64 = 30 * 24 * 3600;
const ABSOLUTE_SECONDS: i64 = 90 * 24 * 3600;
const API_KEY_DEFAULT_EXPIRY: i64 = 90 * 24 * 3600;
const MIN_PASSWORD_LENGTH: usize = 12;
const RECOVERY_CODE_COUNT: usize = 8;
const VERIFICATION_SECONDS: i64 = 24 * 3600;
const LOGIN_MAX_ATTEMPTS: u32 = 10;
const LOGIN_WINDOW_SECONDS: i64 = 15 * 60;
const MAX_TRACKED_LOGIN_KEYS: usize = 10_000;
const MAX_PASSWORD_LENGTH: usize = 1024;

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
		if attempts.len() >= MAX_TRACKED_LOGIN_KEYS && !attempts.contains_key(key) {
			attempts.retain(|_, (_, start)| now - *start < LOGIN_WINDOW_SECONDS);
			if attempts.len() >= MAX_TRACKED_LOGIN_KEYS {
				return;
			}
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

fn dummy_hash() -> &'static str {
	static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
	HASH.get_or_init(|| password::hash_password("not a real password").unwrap_or_default())
}

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/auth/register", post(credentials::register))
		.route("/v1/auth/session", post(credentials::login).delete(credentials::logout))
		.route("/v1/auth/me", get(credentials::me).delete(admin::delete_self))
		.route("/v1/auth/keys", get(api_keys::list).post(api_keys::create))
		.route("/v1/auth/keys/{id}", delete(api_keys::revoke))
		.route("/v1/auth/password", post(recovery::change_password))
		.route("/v1/auth/recovery-codes", post(recovery::issue_recovery_codes))
		.route("/v1/auth/recover", post(recovery::recover))
		.route("/v1/auth/users", get(admin::list_accounts).post(admin::create_account))
		.route("/v1/auth/verify-email", post(verification::verify_email))
		.route("/v1/auth/verify-email/resend", post(verification::resend_verification))
		.route("/v1/auth/users/reset-password", post(recovery::reset_password))
		.route("/v1/auth/users/{id}", delete(admin::delete_account))
		.route("/v1/auth/export", get(admin::export_account))
}

#[cfg(test)]
mod limiter_tests {
	use super::{LoginLimiter, MAX_TRACKED_LOGIN_KEYS};

	#[test]
	fn login_limiter_does_not_grow_past_its_cap() {
		let limiter = LoginLimiter::new();
		for index in 0..=MAX_TRACKED_LOGIN_KEYS {
			limiter.record_failure(&format!("key-{index}"), 100);
		}
		assert_eq!(limiter.attempts.lock().expect("login limiter").len(), MAX_TRACKED_LOGIN_KEYS);
	}
}
