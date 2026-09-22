use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::now;
use crate::auth::AuthenticatedUser;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/follows", get(list_follows))
		.route(
			"/v1/follows/{project_id}",
			post(follow).delete(axum::routing::delete(unfollow)),
		)
		.route("/v1/notifications", get(list_notifications))
		.route("/v1/notifications/read-all", post(read_all))
		.route("/v1/notifications/{id}/read", post(read_one))
}

#[derive(Serialize)]
struct NotificationView {
	id: String,
	project_id: String,
	event_kind: String,
	object: Option<String>,
	feed_seq: Option<i64>,
	created_at: i64,
	read: bool,
}

async fn follow(State(state): State<AppState>, user: AuthenticatedUser, Path(project_id): Path<String>) -> Response {
	match state.metadata.project(&project_id).await {
		Ok(Some(_)) => {}
		Ok(None) => return (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => return storage_error(error),
	}
	match state.metadata.follow(&user.user_id, &project_id, now()).await {
		Ok(()) => (StatusCode::CREATED, Json(serde_json::json!({ "project_id": project_id }))).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn unfollow(State(state): State<AppState>, user: AuthenticatedUser, Path(project_id): Path<String>) -> Response {
	match state.metadata.unfollow(&user.user_id, &project_id).await {
		Ok(true) => StatusCode::NO_CONTENT.into_response(),
		Ok(false) => (StatusCode::NOT_FOUND, "not following").into_response(),
		Err(error) => storage_error(error),
	}
}

async fn list_follows(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	match state.metadata.follows(&user.user_id).await {
		Ok(projects) => Json(projects).into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
struct NotificationQuery {
	#[serde(default)]
	unread: Option<bool>,
}

async fn list_notifications(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Query(query): Query<NotificationQuery>,
) -> Response {
	match state
		.metadata
		.notifications(&user.user_id, query.unread.unwrap_or(false), 100)
		.await
	{
		Ok(rows) => Json(
			rows.into_iter()
				.map(|row| NotificationView {
					id: row.id,
					project_id: row.project_id,
					event_kind: row.event_kind,
					object: row.object_digest.map(|digest| format!("gd:sha256:{}", hex::encode(digest))),
					feed_seq: row.feed_seq,
					created_at: row.created_at,
					read: row.read_at.is_some(),
				})
				.collect::<Vec<_>>(),
		)
		.into_response(),
		Err(error) => storage_error(error),
	}
}

async fn read_one(State(state): State<AppState>, user: AuthenticatedUser, Path(id): Path<String>) -> Response {
	match state.metadata.mark_notification_read(&user.user_id, &id, now()).await {
		Ok(true) => StatusCode::NO_CONTENT.into_response(),
		Ok(false) => (StatusCode::NOT_FOUND, "no such notification").into_response(),
		Err(error) => storage_error(error),
	}
}

async fn read_all(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	match state.metadata.mark_all_notifications_read(&user.user_id, now()).await {
		Ok(()) => StatusCode::NO_CONTENT.into_response(),
		Err(error) => storage_error(error),
	}
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "notification store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
