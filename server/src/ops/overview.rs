use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};

use crate::auth::AuthenticatedUser;
use crate::registry::storage_error;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
	Router::new().route("/v1/admin/overview", get(overview))
}

async fn overview(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if !user.allows("directory:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let metadata = &state.metadata;
	let counts = async {
		Ok::<_, sqlx::Error>(serde_json::json!({
			"projects": metadata.table_count("projects").await?,
			"definitions": metadata.table_count("definitions").await?,
			"accounts": metadata.table_count("users").await?,
			"unverified_accounts": metadata
				.scalar_count("SELECT COUNT(*) FROM users WHERE verified_at IS NULL")
				.await?,
			"pending_submissions": metadata
				.scalar_count("SELECT COUNT(*) FROM submissions WHERE state IN ('submitted', 'under_review')")
				.await?,
			"followed_homes": metadata.table_count("subscriptions").await?,
			"advisories": metadata.table_count("advisories").await?,
		}))
	}
	.await;
	match counts {
		Ok(view) => Json(view).into_response(),
		Err(error) => storage_error(error),
	}
}
