use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_model::moderation::{ImpersonationReport, ReportStatus};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::auth::AuthenticatedUser;
use crate::db::MetadataStore;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/impersonation-reports", get(list_reports).post(record_report))
		.route("/v1/impersonation-reports/{id}", get(get_report))
}

#[derive(Debug, Clone)]
pub struct ReportRow {
	pub id: String,
	pub report: ImpersonationReport,
	pub recorded_by: String,
	pub recorded_at: i64,
}

impl MetadataStore {
	pub async fn record_impersonation_report(
		&self,
		id: &str,
		report: &ImpersonationReport,
		recorded_by: &str,
		recorded_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO impersonation_reports (id, claim_kind, claimant_ref, target_project_id, target_handle, evidence_ref,
			 status, decided_at, recorded_by, recorded_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
		)
		.bind(id)
		.bind(&report.claim_kind)
		.bind(&report.claimant_ref)
		.bind(report.target_project_id.as_deref())
		.bind(report.target_handle.as_deref())
		.bind(&report.evidence_ref)
		.bind(report.status.as_str())
		.bind(report.decided_at)
		.bind(recorded_by)
		.bind(recorded_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn impersonation_report(&self, id: &str) -> Result<Option<ReportRow>, sqlx::Error> {
		let row = sqlx::query(sqlx::AssertSqlSafe(format!("{REPORT_SELECT} WHERE id = $1")))
			.bind(id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.and_then(report_row))
	}

	pub async fn impersonation_reports_for(
		&self,
		project_id: Option<&str>,
		handle: Option<&str>,
	) -> Result<Vec<ReportRow>, sqlx::Error> {
		let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
			"{REPORT_SELECT} WHERE ($1 IS NULL OR target_project_id = $1) AND ($2 IS NULL OR target_handle = $2)
		 ORDER BY recorded_at DESC, id ASC"
		)))
		.bind(project_id)
		.bind(handle)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().filter_map(report_row).collect())
	}
}

const REPORT_SELECT: &str = "SELECT id, claim_kind, claimant_ref, target_project_id, target_handle, evidence_ref, status, decided_at, recorded_by, recorded_at FROM impersonation_reports";

fn report_row(row: sqlx::any::AnyRow) -> Option<ReportRow> {
	let status: String = row.get("status");
	Some(ReportRow {
		id: row.get("id"),
		report: ImpersonationReport {
			claim_kind: row.get("claim_kind"),
			claimant_ref: row.get("claimant_ref"),
			target_project_id: row.get("target_project_id"),
			target_handle: row.get("target_handle"),
			evidence_ref: row.get("evidence_ref"),
			status: ReportStatus::parse(&status)?,
			decided_at: row.get("decided_at"),
		},
		recorded_by: row.get("recorded_by"),
		recorded_at: row.get("recorded_at"),
	})
}

#[derive(Serialize)]
struct ReportView {
	id: String,
	claim_kind: String,
	claimant_ref: String,
	target_project_id: Option<String>,
	target_handle: Option<String>,
	evidence_ref: String,
	status: String,
	decided_at: Option<i64>,
	recorded_by: String,
	recorded_at: i64,
}

fn view(row: ReportRow) -> ReportView {
	ReportView {
		id: row.id,
		claim_kind: row.report.claim_kind,
		claimant_ref: row.report.claimant_ref,
		target_project_id: row.report.target_project_id,
		target_handle: row.report.target_handle,
		evidence_ref: row.report.evidence_ref,
		status: row.report.status.as_str().to_string(),
		decided_at: row.report.decided_at,
		recorded_by: row.recorded_by,
		recorded_at: row.recorded_at,
	}
}

#[derive(Deserialize)]
struct RecordReport {
	claim_kind: String,
	claimant_ref: String,
	target_project_id: Option<String>,
	target_handle: Option<String>,
	evidence_ref: String,
	#[serde(default = "open_status")]
	status: String,
	decided_at: Option<i64>,
}

fn open_status() -> String {
	ReportStatus::Open.as_str().to_string()
}

async fn record_report(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<RecordReport>,
) -> Response {
	if !user.allows("directory:manage") {
		return forbidden();
	}
	let Some(status) = ReportStatus::parse(&request.status) else {
		return (StatusCode::BAD_REQUEST, format!("unknown status `{}`", request.status)).into_response();
	};
	let row = ReportRow {
		id: new_id(),
		report: ImpersonationReport {
			claim_kind: request.claim_kind,
			claimant_ref: request.claimant_ref,
			target_project_id: request.target_project_id,
			target_handle: request.target_handle,
			evidence_ref: request.evidence_ref,
			status,
			decided_at: request.decided_at,
		},
		recorded_by: user.user_id.clone(),
		recorded_at: now(),
	};
	if let Err(message) = row.report.validate() {
		return (StatusCode::BAD_REQUEST, message).into_response();
	}
	if let Err(error) = state
		.metadata
		.record_impersonation_report(&row.id, &row.report, &row.recorded_by, row.recorded_at)
		.await
	{
		return storage_error(error);
	}
	(StatusCode::CREATED, Json(view(row))).into_response()
}

#[derive(Deserialize)]
struct ListQuery {
	project: Option<String>,
	handle: Option<String>,
}

async fn list_reports(State(state): State<AppState>, user: AuthenticatedUser, Query(query): Query<ListQuery>) -> Response {
	if !user.allows("directory:manage") {
		return forbidden();
	}
	match state
		.metadata
		.impersonation_reports_for(query.project.as_deref(), query.handle.as_deref())
		.await
	{
		Ok(rows) => Json(rows.into_iter().map(view).collect::<Vec<_>>()).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn get_report(State(state): State<AppState>, Path(id): Path<String>, user: AuthenticatedUser) -> Response {
	if !user.allows("directory:manage") {
		return forbidden();
	}
	match state.metadata.impersonation_report(&id).await {
		Ok(Some(row)) => Json(view(row)).into_response(),
		Ok(None) => (StatusCode::NOT_FOUND, "no such report").into_response(),
		Err(error) => storage_error(error),
	}
}

fn forbidden() -> Response {
	(StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response()
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

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "impersonation report store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
