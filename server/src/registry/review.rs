use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use moraine_model::feed::FeedEntry;
use moraine_model::moderation::{REASON_TAXONOMY_VERSION, ReasonCode};
use moraine_model::signed::SignedObject;
use serde::{Deserialize, Serialize};

use crate::auth::AuthenticatedUser;
use crate::db::{ReviewDecisionRow, SubmissionRow};
use crate::registry::{ingest_feed, prepare_feed};
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/submissions", get(my_submissions).post(submit))
		.route("/v1/submissions/{id}", get(submission_detail))
		.route("/v1/review-queue", get(queue))
		.route("/v1/submissions/{id}/assign", post(assign))
		.route("/v1/submissions/{id}/review", post(review))
}

#[derive(Serialize)]
struct SubmissionView {
	id: String,
	project_id: String,
	object: String,
	entry: String,
	state: String,
	assigned_to: Option<String>,
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
						assigned_to: None,
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
		assigned_to: None,
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
	let view = submission_view(submission);
	let decisions = decision_views(decisions);
	Json(SubmissionDetail {
		submission: view,
		decisions,
	})
	.into_response()
}

fn decision_views(decisions: Vec<ReviewDecisionRow>) -> Vec<DecisionView> {
	decisions
		.into_iter()
		.map(|decision| DecisionView {
			decision: decision.decision,
			reason_code: decision.reason_code,
			reason_taxonomy_version: decision.reason_taxonomy_version,
			reviewer_id: decision.reviewer_id,
			decided_at: decision.decided_at,
			appeal_route: decision.appeal_route,
		})
		.collect()
}

#[derive(Deserialize)]
struct SubmissionPage {
	#[serde(default)]
	limit: Option<u32>,
	#[serde(default)]
	cursor: Option<String>,
}

fn parse_cursor(raw: Option<&str>) -> Result<Option<(i64, String)>, Box<Response>> {
	let Some(raw) = raw else {
		return Ok(None);
	};
	let Some((created, id)) = raw.split_once(':') else {
		return Err(Box::new(
			(StatusCode::BAD_REQUEST, "cursor must be `created_at:id`").into_response(),
		));
	};
	let Ok(created) = created.parse::<i64>() else {
		return Err(Box::new(
			(StatusCode::BAD_REQUEST, "cursor must be `created_at:id`").into_response(),
		));
	};
	Ok(Some((created, id.to_string())))
}

async fn my_submissions(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Query(params): Query<SubmissionPage>,
) -> Response {
	let limit = params.limit.unwrap_or(50).clamp(1, 200) as i64;
	let cursor = match parse_cursor(params.cursor.as_deref()) {
		Ok(cursor) => cursor,
		Err(response) => return *response,
	};
	let cursor = cursor.as_ref().map(|(created, id)| (*created, id.as_str()));
	let rows = match state.metadata.submissions_by_submitter(&user.user_id, limit, cursor).await {
		Ok(rows) => rows,
		Err(error) => return storage_error(error),
	};
	let mut details = Vec::with_capacity(rows.len());
	for row in rows {
		let decisions = match state.metadata.decisions_for(&row.id).await {
			Ok(decisions) => decision_views(decisions),
			Err(error) => return storage_error(error),
		};
		details.push(SubmissionDetail {
			submission: submission_view(row),
			decisions,
		});
	}
	Json(details).into_response()
}

fn submission_view(row: SubmissionRow) -> SubmissionView {
	SubmissionView {
		id: row.id,
		project_id: row.project_id,
		object: id_for(&row.object_digest),
		entry: id_for(&row.entry_digest),
		state: row.state,
		assigned_to: row.assigned_to,
		submitted_by: row.submitted_by,
		created_at: row.created_at,
		updated_at: row.updated_at,
	}
}

#[derive(Deserialize)]
struct QueuePage {
	#[serde(default)]
	limit: Option<u32>,
	#[serde(default)]
	cursor: Option<String>,
}

async fn queue(State(state): State<AppState>, user: AuthenticatedUser, Query(params): Query<QueuePage>) -> Response {
	if !user.allows("submissions:review") {
		return forbidden();
	}
	let limit = params.limit.unwrap_or(100).clamp(1, 200) as i64;
	let cursor = match parse_cursor(params.cursor.as_deref()) {
		Ok(cursor) => cursor,
		Err(response) => return *response,
	};
	let cursor = cursor.as_ref().map(|(created, id)| (*created, id.as_str()));
	match state.metadata.open_submissions(limit, cursor).await {
		Ok(rows) => Json(rows.into_iter().map(submission_view).collect::<Vec<_>>()).into_response(),
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

async fn assign(State(state): State<AppState>, user: AuthenticatedUser, Path(id): Path<String>) -> Response {
	if !user.allows("submissions:review") {
		return forbidden();
	}
	match state.metadata.submission(&id).await {
		Ok(Some(submission)) if submission.state == "submitted" => {}
		Ok(Some(_)) => return (StatusCode::CONFLICT, "submission is already assigned or decided").into_response(),
		Ok(None) => return (StatusCode::NOT_FOUND, "no such submission").into_response(),
		Err(error) => return storage_error(error),
	}
	match state.metadata.assign_submission(&id, &user.user_id, now()).await {
		Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "state": "under_review" }))).into_response(),
		Ok(false) => (StatusCode::CONFLICT, "submission is already assigned or decided").into_response(),
		Err(error) => storage_error(error),
	}
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
	let assigned_to_caller = submission.state == "under_review" && submission.assigned_to.as_deref() == Some(&user.user_id);
	if submission.state != "submitted" && !assigned_to_caller {
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
