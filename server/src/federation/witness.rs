use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::storage_error;
use crate::auth::AuthenticatedUser;
use crate::registry;
use crate::routes::AppState;

pub(super) async fn observe(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Path(project_id): Path<String>,
) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let observations = match state.metadata.witness_observations(&project_id).await {
		Ok(observations) => observations,
		Err(error) => return storage_error(error),
	};
	let conflicts = registry::witness::witness_conflicts(&observations);
	let observations = observations
		.iter()
		.map(|row| {
			serde_json::json!({
				"source_home": row.source_home,
				"sequence": row.sequence,
				"head_entry": row.head_entry,
				"observed_at": row.observed_at,
			})
		})
		.collect::<Vec<_>>();
	let conflicts = conflicts
		.iter()
		.map(|conflict| {
			serde_json::json!({
				"sequence": conflict.sequence,
				"entries": conflict.entries,
				"homes": conflict.homes,
			})
		})
		.collect::<Vec<_>>();
	Json(serde_json::json!({
		"project_id": project_id,
		"observations": observations,
		"conflicts": conflicts,
	}))
	.into_response()
}
