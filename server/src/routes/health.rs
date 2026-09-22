use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};

use crate::capability::Capability;
use crate::routes::AppState;

pub(super) fn routes() -> Router<AppState> {
	Router::new()
		.route("/.well-known/mod-registry", get(well_known))
		.route("/healthz", get(health))
		.route("/readyz", get(ready))
}

async fn health() -> &'static str {
	"ok"
}

async fn well_known(State(state): State<AppState>) -> Json<Capability> {
	Json((*state.capability).clone())
}

async fn ready(State(state): State<AppState>) -> Response {
	let probe = [0u8; 32];
	match state.store.size(&probe).await {
		Ok(_) => (StatusCode::OK, "ok").into_response(),
		Err(error) => {
			tracing::warn!(%error, "readiness probe failed");
			(StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response()
		}
	}
}
