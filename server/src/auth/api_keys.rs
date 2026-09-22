use axum::extract::{Json, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::security::{forbidden, new_id, now, random_token, split_scopes, storage_error, token_hash};
use super::session::AuthenticatedUser;
use super::{API_KEY_DEFAULT_EXPIRY, KNOWN_SCOPES};
use crate::auth::accounts::ApiKeyRow;
use crate::routes::AppState;

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

pub(super) async fn list(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if !user.allows("keys:manage") {
		return forbidden();
	}
	match state.metadata.api_keys_for_user(&user.user_id).await {
		Ok(keys) => Json(
			keys.into_iter()
				.map(|key| ApiKeyView {
					id: key.id,
					name: key.name,
					prefix: key.prefix,
					scopes: split_scopes(&key.scopes),
					created_at: key.created_at,
					expires_at: key.expires_at,
					last_used_at: key.last_used_at,
				})
				.collect::<Vec<_>>(),
		)
		.into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
pub(super) struct CreateKey {
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

pub(super) async fn create(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<CreateKey>,
) -> Response {
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
	(
		StatusCode::CREATED,
		Json(CreatedKey {
			id: key.id,
			name: key.name,
			prefix,
			key: secret,
			scopes: request.scopes,
			expires_at,
		}),
	)
		.into_response()
}

pub(super) async fn revoke(State(state): State<AppState>, user: AuthenticatedUser, Path(id): Path<String>) -> Response {
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
