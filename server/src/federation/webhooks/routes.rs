use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::event::EventKind;
use serde::{Deserialize, Serialize};

use super::validation::validate_url;
use super::{new_id, now};
use crate::auth::AuthenticatedUser;
use crate::routes::AppState;

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
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
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
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
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
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	match state.metadata.revoke_webhook(&user.user_id, &id, now()).await {
		Ok(true) => StatusCode::NO_CONTENT.into_response(),
		Ok(false) => (StatusCode::NOT_FOUND, "no such webhook").into_response(),
		Err(error) => storage_error(error),
	}
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "webhook store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
