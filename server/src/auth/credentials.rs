use axum::extract::{Json, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::security::{new_id, now, random_token, storage_error, token_hash};
use super::session::{AuthenticatedUser, ClientIp};
use super::{ABSOLUTE_SECONDS, IDLE_SECONDS, MAX_PASSWORD_LENGTH, MIN_PASSWORD_LENGTH, cookies};
use crate::auth::accounts::SessionRow;
use crate::auth::password;
use crate::routes::AppState;

#[derive(Deserialize)]
pub(super) struct Credentials {
	email: String,
	password: String,
}

#[derive(Serialize)]
struct RegisterReceipt {
	user_id: String,
	verified: bool,
}

#[derive(Serialize)]
struct LoginReceipt {
	user_id: String,
	expires_at: i64,
	csrf: String,
}

pub(super) async fn register(State(state): State<AppState>, Json(credentials): Json<Credentials>) -> Response {
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
	if credentials.password.len() > MAX_PASSWORD_LENGTH {
		return (StatusCode::BAD_REQUEST, "password is too long").into_response();
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
	let verified = if state.capability.email_verification {
		if let Err(error) = super::verification::send_verification(&state, &user_id, &email).await {
			tracing::error!(%error, "the verification email could not be sent");
		}
		false
	} else {
		let _ = state.metadata.set_user_verified(&user_id, now()).await;
		true
	};
	(StatusCode::CREATED, Json(RegisterReceipt { user_id, verified })).into_response()
}

pub(super) async fn login(
	State(state): State<AppState>,
	client: ClientIp,
	Json(credentials): Json<Credentials>,
) -> Response {
	if credentials.password.len() > MAX_PASSWORD_LENGTH {
		return (StatusCode::BAD_REQUEST, "password is too long").into_response();
	}
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
			let _ = password::verify_password(&credentials.password, super::dummy_hash());
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
	cookies::append(&mut response, &cookies::session(&token, cross_origin));
	cookies::append(&mut response, &cookies::csrf(&csrf, cross_origin));
	response
}

pub(super) async fn logout(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if let Some(session_id) = &user.session_id {
		if let Err(error) = state.metadata.revoke_session(session_id, now()).await {
			return storage_error(error);
		}
		state.metrics.record_session_revoked();
	}
	let cross_origin = state.capability.cross_origin();
	let mut response = StatusCode::NO_CONTENT.into_response();
	cookies::append(&mut response, &cookies::clear(super::SESSION_COOKIE, true, cross_origin));
	cookies::append(&mut response, &cookies::clear(super::CSRF_COOKIE, false, cross_origin));
	response
}

#[derive(Serialize)]
struct AccountView {
	user_id: String,
	email: String,
	via: &'static str,
	role: String,
	verified: bool,
	created_at: i64,
}

pub(super) async fn me(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	match state.metadata.user_by_id(&user.user_id).await {
		Ok(Some(record)) => Json(AccountView {
			user_id: record.id,
			email: record.email,
			via: user.via(),
			role: record.role,
			verified: record.verified_at.is_some(),
			created_at: record.created_at,
		})
		.into_response(),
		Ok(None) => (StatusCode::UNAUTHORIZED, "account no longer exists").into_response(),
		Err(error) => storage_error(error),
	}
}
