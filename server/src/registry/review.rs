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
use sqlx::Row;

use crate::auth::AuthenticatedUser;
use crate::db::MetadataStore;
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
	if let Some(response) = crate::auth::verified_or_error(&state, &user.user_id).await {
		return response;
	}
	if let Some(message) = crate::registry::sanctions::publishing_block(&state, &user.user_id).await {
		return (StatusCode::FORBIDDEN, message).into_response();
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
	let progressive_grant = if state.capability.publishing == "progressive" {
		match crate::registry::grants::release_scope(&state, &prepared.entry.object_digest).await {
			Some((game_id, release_kind)) => match state
				.metadata
				.active_publication_grant(&project_id, &user.user_id, &game_id, &release_kind, now())
				.await
			{
				Ok(grant) => grant.is_some(),
				Err(error) => return storage_error(error),
			},
			None => false,
		}
	} else {
		false
	};
	if state.capability.is_open() || progressive_grant {
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
			if state.capability.publishing == "progressive"
				&& let Some((game_id, release_kind)) =
					crate::registry::grants::release_scope(&state, &submission.object_digest).await
				&& state
					.metadata
					.active_publication_grant(
						&submission.project_id,
						&submission.submitted_by,
						&game_id,
						&release_kind,
						current,
					)
					.await
					.ok()
					.flatten()
					.is_none()
			{
				let grant = crate::registry::grants::PublicationGrantRow {
					id: crate::registry::grants::new_id(),
					project_id: submission.project_id.clone(),
					principal_kind: "user".to_string(),
					principal_id: submission.submitted_by.clone(),
					game_id,
					release_kinds: release_kind,
					artifact_kinds: "*".to_string(),
					issued_from_review: id.clone(),
					policy_version: "progressive-v1".to_string(),
					issued_at: current,
					expires_at: None,
					suspended_at: None,
					revoked_at: None,
					reason_code: None,
				};
				if let Err(error) = state.metadata.issue_publication_grant(&grant).await {
					tracing::error!(%error, project = %submission.project_id, "progressive publication grant could not be issued");
				}
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

#[derive(Debug, Clone)]
pub struct SubmissionRow {
	pub id: String,
	pub project_id: String,
	pub object_digest: Vec<u8>,
	pub entry_digest: Vec<u8>,
	pub entry_wire: Vec<u8>,
	pub state: String,
	pub assigned_to: Option<String>,
	pub submitted_by: String,
	pub created_at: i64,
	pub updated_at: i64,
}
#[derive(Debug, Clone)]
pub struct ReviewDecisionRow {
	pub id: String,
	pub submission_id: String,
	pub object_digest: Vec<u8>,
	pub reviewer_id: String,
	pub decision: String,
	pub reason_code: Option<String>,
	pub reason_taxonomy_version: u32,
	pub decided_at: i64,
	pub appeal_route: Option<String>,
}

impl MetadataStore {
	pub async fn create_submission(&self, submission: &SubmissionRow) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO submissions (id, project_id, object_digest, entry_digest, entry_wire, state, submitted_by, created_at, updated_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)",
		)
		.bind(&submission.id)
		.bind(&submission.project_id)
		.bind(&submission.object_digest)
		.bind(&submission.entry_digest)
		.bind(&submission.entry_wire)
		.bind(&submission.state)
		.bind(&submission.submitted_by)
		.bind(submission.created_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn submission(&self, id: &str) -> Result<Option<SubmissionRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT id, project_id, object_digest, entry_digest, entry_wire, state, assigned_to, submitted_by, created_at, updated_at
			 FROM submissions WHERE id = $1",
		)
		.bind(id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(submission_row))
	}

	pub async fn open_submissions(
		&self,
		limit: i64,
		cursor: Option<(i64, &str)>,
	) -> Result<Vec<SubmissionRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, project_id, object_digest, entry_digest, entry_wire, state, assigned_to, submitted_by, created_at, updated_at
			 FROM submissions WHERE state IN ('submitted', 'under_review')
			 AND (created_at > $1 OR (created_at = $1 AND ($3 = 0 OR id > $2)))
			 ORDER BY created_at ASC, id ASC LIMIT $4",
		)
		.bind(cursor.map(|(created, _)| created).unwrap_or(i64::MIN))
		.bind(cursor.map(|(_, id)| id).unwrap_or(""))
		.bind(i64::from(cursor.is_some()))
		.bind(limit)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(submission_row).collect())
	}

	pub async fn submissions_by_submitter(
		&self,
		user_id: &str,
		limit: i64,
		cursor: Option<(i64, &str)>,
	) -> Result<Vec<SubmissionRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, project_id, object_digest, entry_digest, entry_wire, state, assigned_to, submitted_by, created_at, updated_at
			 FROM submissions WHERE submitted_by = $1
			 AND (created_at < $2 OR (created_at = $2 AND ($4 = 0 OR id < $3)))
			 ORDER BY created_at DESC, id DESC LIMIT $5",
		)
		.bind(user_id)
		.bind(cursor.map(|(created, _)| created).unwrap_or(i64::MAX))
		.bind(cursor.map(|(_, id)| id).unwrap_or(""))
		.bind(i64::from(cursor.is_some()))
		.bind(limit)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(submission_row).collect())
	}

	pub async fn assign_submission(&self, id: &str, reviewer_id: &str, updated_at: i64) -> Result<bool, sqlx::Error> {
		let result = sqlx::query(
			"UPDATE submissions SET state = 'under_review', assigned_to = $1, updated_at = $2 WHERE id = $3 AND state = 'submitted'",
		)
		.bind(reviewer_id)
		.bind(updated_at)
		.bind(id)
		.execute(&self.pool)
		.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn set_submission_state(&self, id: &str, state: &str, updated_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE submissions SET state = $1, updated_at = $2 WHERE id = $3")
			.bind(state)
			.bind(updated_at)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn insert_decision(&self, decision: &ReviewDecisionRow) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO review_decisions (id, submission_id, object_digest, reviewer_id, decision, reason_code, reason_taxonomy_version, decided_at, appeal_route)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
		)
		.bind(&decision.id)
		.bind(&decision.submission_id)
		.bind(&decision.object_digest)
		.bind(&decision.reviewer_id)
		.bind(&decision.decision)
		.bind(&decision.reason_code)
		.bind(i64::from(decision.reason_taxonomy_version))
		.bind(decision.decided_at)
		.bind(&decision.appeal_route)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn decisions_for(&self, submission_id: &str) -> Result<Vec<ReviewDecisionRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, submission_id, object_digest, reviewer_id, decision, reason_code, reason_taxonomy_version, decided_at, appeal_route
			 FROM review_decisions WHERE submission_id = $1 ORDER BY decided_at ASC",
		)
		.bind(submission_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| ReviewDecisionRow {
				id: row.get("id"),
				submission_id: row.get("submission_id"),
				object_digest: row.get("object_digest"),
				reviewer_id: row.get("reviewer_id"),
				decision: row.get("decision"),
				reason_code: row.get("reason_code"),
				reason_taxonomy_version: row.get::<i64, _>("reason_taxonomy_version") as u32,
				decided_at: row.get("decided_at"),
				appeal_route: row.get("appeal_route"),
			})
			.collect())
	}
}

fn submission_row(row: sqlx::any::AnyRow) -> SubmissionRow {
	SubmissionRow {
		id: row.get("id"),
		project_id: row.get("project_id"),
		object_digest: row.get("object_digest"),
		entry_digest: row.get("entry_digest"),
		entry_wire: row.get("entry_wire"),
		state: row.get("state"),
		assigned_to: row.get("assigned_to"),
		submitted_by: row.get("submitted_by"),
		created_at: row.get("created_at"),
		updated_at: row.get("updated_at"),
	}
}
