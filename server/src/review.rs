use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_model::feed::FeedEntry;
use moraine_model::moderation::{REASON_TAXONOMY_VERSION, ReasonCode};
use moraine_model::signed::SignedObject;
use serde::{Deserialize, Serialize};

use crate::auth::AuthenticatedUser;
use crate::registry::{ingest_feed, prepare_feed};
use crate::routes::AppState;
use crate::store::{ReviewDecisionRow, SubmissionRow};

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/submissions", post(submit))
		.route("/v1/submissions/{id}", get(submission_detail))
		.route("/v1/review-queue", get(queue))
		.route("/v1/submissions/{id}/review", post(review))
}

#[derive(Serialize)]
struct SubmissionView {
	id: String,
	project_id: String,
	object: String,
	entry: String,
	state: String,
	submitted_by: String,
	created_at: i64,
	updated_at: i64,
}

#[derive(Serialize)]
struct SubmissionReceipt {
	id: String,
	project_id: String,
	state: String,
}

#[derive(Serialize)]
struct ReviewReceipt {
	id: String,
	state: String,
	seq: Option<i64>,
	entry: Option<String>,
}

async fn submit(State(state): State<AppState>, user: AuthenticatedUser, body: Bytes) -> Response {
	if !user.allows("submissions:write") {
		return forbidden();
	}
	let Ok(signed) = SignedObject::<FeedEntry>::from_bytes(&body) else {
		return (StatusCode::BAD_REQUEST, "invalid feed entry").into_response();
	};
	let project_id = signed.payload.project_id.clone();
	let prepared = match prepare_feed(&state, &project_id, &body).await {
		Ok(prepared) => prepared,
		Err(response) => return *response,
	};

	let id = new_id();
	let current = now();
	if state.capability.is_open() {
		match ingest_feed(&state, &project_id, &body).await {
			Ok((seq, entry)) => {
				if let Err(error) = state
					.metadata
					.create_submission(&SubmissionRow {
						id: id.clone(),
						project_id: project_id.clone(),
						object_digest: prepared.entry.object_digest.clone(),
						entry_digest: prepared.object.digest.to_vec(),
						entry_wire: body.to_vec(),
						state: "auto-accepted".to_string(),
						submitted_by: user.user_id.clone(),
						created_at: current,
						updated_at: current,
					})
					.await
				{
					return storage_error(error);
				}
				let response = ReviewReceipt {
					id,
					state: "auto-accepted".to_string(),
					seq: Some(seq),
					entry: Some(entry),
				};
				return (StatusCode::CREATED, Json(response)).into_response();
			}
			Err(response) => return *response,
		}
	}

	let submission = SubmissionRow {
		id: id.clone(),
		project_id: project_id.clone(),
		object_digest: prepared.entry.object_digest.clone(),
		entry_digest: prepared.object.digest.to_vec(),
		entry_wire: body.to_vec(),
		state: "submitted".to_string(),
		submitted_by: user.user_id,
		created_at: current,
		updated_at: current,
	};
	if let Err(error) = state.metadata.create_submission(&submission).await {
		return storage_error(error);
	}
	(
		StatusCode::ACCEPTED,
		Json(SubmissionReceipt {
			id,
			project_id,
			state: "submitted".to_string(),
		}),
	)
		.into_response()
}

#[derive(Serialize)]
struct DecisionView {
	decision: String,
	reason_code: Option<String>,
	reason_taxonomy_version: u32,
	reviewer_id: String,
	decided_at: i64,
	appeal_route: Option<String>,
}

#[derive(Serialize)]
struct SubmissionDetail {
	submission: SubmissionView,
	decisions: Vec<DecisionView>,
}

async fn submission_detail(State(state): State<AppState>, user: AuthenticatedUser, Path(id): Path<String>) -> Response {
	let submission = match state.metadata.submission(&id).await {
		Ok(Some(submission)) => submission,
		Ok(None) => return (StatusCode::NOT_FOUND, "no such submission").into_response(),
		Err(error) => return storage_error(error),
	};
	if submission.submitted_by != user.user_id && !user.allows("submissions:review") {
		return forbidden();
	}
	let decisions = match state.metadata.decisions_for(&id).await {
		Ok(decisions) => decisions,
		Err(error) => return storage_error(error),
	};
	let view = SubmissionView {
		id: submission.id,
		project_id: submission.project_id,
		object: id_for(&submission.object_digest),
		entry: id_for(&submission.entry_digest),
		state: submission.state,
		submitted_by: submission.submitted_by,
		created_at: submission.created_at,
		updated_at: submission.updated_at,
	};
	let decisions = decisions
		.into_iter()
		.map(|decision| DecisionView {
			decision: decision.decision,
			reason_code: decision.reason_code,
			reason_taxonomy_version: decision.reason_taxonomy_version,
			reviewer_id: decision.reviewer_id,
			decided_at: decision.decided_at,
			appeal_route: decision.appeal_route,
		})
		.collect();
	Json(SubmissionDetail {
		submission: view,
		decisions,
	})
	.into_response()
}

async fn queue(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if !user.allows("submissions:review") {
		return forbidden();
	}
	match state.metadata.submissions_in_state("submitted", 100).await {
		Ok(rows) => {
			let view: Vec<SubmissionView> = rows
				.into_iter()
				.map(|row| SubmissionView {
					id: row.id,
					project_id: row.project_id,
					object: id_for(&row.object_digest),
					entry: id_for(&row.entry_digest),
					state: row.state,
					submitted_by: row.submitted_by,
					created_at: row.created_at,
					updated_at: row.updated_at,
				})
				.collect();
			Json(view).into_response()
		}
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
struct ReviewRequest {
	decision: String,
	#[serde(default)]
	reason_code: Option<String>,
	#[serde(default)]
	appeal_route: Option<String>,
}

async fn review(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Path(id): Path<String>,
	Json(request): Json<ReviewRequest>,
) -> Response {
	if !user.allows("submissions:review") {
		return forbidden();
	}
	let submission = match state.metadata.submission(&id).await {
		Ok(Some(submission)) => submission,
		Ok(None) => return (StatusCode::NOT_FOUND, "no such submission").into_response(),
		Err(error) => return storage_error(error),
	};
	if submission.state != "submitted" {
		return (StatusCode::CONFLICT, "submission already decided").into_response();
	}

	let current = now();
	let decision_row = |decision: &str, reason: Option<String>| ReviewDecisionRow {
		id: new_id(),
		submission_id: id.clone(),
		object_digest: submission.object_digest.clone(),
		reviewer_id: user.user_id.clone(),
		decision: decision.to_string(),
		reason_code: reason,
		reason_taxonomy_version: REASON_TAXONOMY_VERSION,
		decided_at: current,
		appeal_route: request.appeal_route.clone(),
	};

	match request.decision.as_str() {
		"accept" => {
			let (seq, entry) = match ingest_feed(&state, &submission.project_id, &submission.entry_wire).await {
				Ok(result) => result,
				Err(response) => return *response,
			};
			if let Err(error) = state.metadata.set_submission_state(&id, "accepted", current).await {
				return storage_error(error);
			}
			if let Err(error) = state.metadata.insert_decision(&decision_row("accept", None)).await {
				return storage_error(error);
			}
			let response = ReviewReceipt {
				id,
				state: "accepted".to_string(),
				seq: Some(seq),
				entry: Some(entry),
			};
			Json(response).into_response()
		}
		"reject" | "quarantine" => {
			let Some(reason_code) = request.reason_code.as_deref() else {
				return (StatusCode::BAD_REQUEST, "a reason code is required").into_response();
			};
			if ReasonCode::parse(reason_code).is_none() {
				return (StatusCode::BAD_REQUEST, "unknown reason code").into_response();
			}
			let state_name = if request.decision == "reject" {
				"rejected"
			} else {
				"quarantined"
			};
			if let Err(error) = state.metadata.set_submission_state(&id, state_name, current).await {
				return storage_error(error);
			}
			let decision = decision_row(&request.decision, Some(reason_code.to_string()));
			if let Err(error) = state.metadata.insert_decision(&decision).await {
				return storage_error(error);
			}
			Json(ReviewReceipt {
				id,
				state: state_name.to_string(),
				seq: None,
				entry: None,
			})
			.into_response()
		}
		_ => (StatusCode::BAD_REQUEST, "decision must be accept, reject, or quarantine").into_response(),
	}
}

fn id_for(digest: &[u8]) -> String {
	format!("gd:sha256:{}", hex::encode(digest))
}

fn new_id() -> String {
	let mut bytes = [0u8; 16];
	if getrandom::fill(&mut bytes).is_err() {
		panic!("operating system randomness is unavailable");
	}
	hex::encode(bytes)
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

fn forbidden() -> Response {
	(StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response()
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "review store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
