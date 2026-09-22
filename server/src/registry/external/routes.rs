use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::store::{ExternalClaimRow, ExternalFileRow, ExternalProjectRow};
use super::{new_id, now};
use crate::auth::AuthenticatedUser;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
	Router::new()
		.route(
			"/v1/external-projects/{provider}/{external_id}",
			get(get_project).put(upsert_project),
		)
		.route("/v1/external-projects/{provider}/{external_id}/claim", post(create_claim))
		.route("/v1/external-project-claims", get(list_claims))
		.route("/v1/external-project-claims/{id}/review", post(review_claim))
}

#[derive(Deserialize)]
struct UpsertRequest {
	source_class: String,
	canonical_source_url: String,
	observed_profile: Value,
	files: Vec<FileRequest>,
	observed_at: i64,
	source_state: String,
	bridge_id: String,
	bridge_version: String,
}

#[derive(Deserialize)]
struct FileRequest {
	external_file_id: String,
	source_url: String,
	digest: Option<String>,
	size: Option<i64>,
	#[serde(default)]
	metadata: Value,
	observed_at: i64,
	deleted_at: Option<i64>,
}

#[derive(Serialize)]
struct ProjectView {
	provider: String,
	external_project_id: String,
	source_class: String,
	canonical_source_url: String,
	observed_profile: Value,
	files: Vec<FileView>,
	observed_at: i64,
	last_synced_at: i64,
	source_state: String,
	bridge_id: String,
	bridge_version: String,
	linked_native_project_id: Option<String>,
}

#[derive(Serialize)]
struct FileView {
	external_file_id: String,
	source_url: String,
	digest: Option<String>,
	size: Option<i64>,
	metadata: Value,
	observed_at: i64,
	deleted_at: Option<i64>,
}

async fn get_project(State(state): State<AppState>, Path((provider, external_id)): Path<(String, String)>) -> Response {
	match state.metadata.external_project(&provider, &external_id).await {
		Ok(Some((project, files))) => Json(project_view(project, files)).into_response(),
		Ok(None) => (StatusCode::NOT_FOUND, "no such external project").into_response(),
		Err(error) => storage_error(error),
	}
}

async fn upsert_project(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Path((provider, external_id)): Path<(String, String)>,
	Json(request): Json<UpsertRequest>,
) -> Response {
	if !user.allows("federation:manage") {
		return forbidden();
	}
	if let Err(error) = validate_project(&provider, &external_id, &request) {
		return (StatusCode::BAD_REQUEST, error).into_response();
	}
	let files = match request.files.into_iter().map(parse_file).collect::<Result<Vec<_>, _>>() {
		Ok(files) => files,
		Err(error) => return (StatusCode::BAD_REQUEST, error).into_response(),
	};
	let now = now();
	let project = ExternalProjectRow {
		provider,
		external_project_id: external_id,
		source_class: request.source_class,
		canonical_source_url: request.canonical_source_url,
		observed_profile: request.observed_profile,
		observed_at: request.observed_at,
		last_synced_at: now,
		source_state: request.source_state,
		bridge_id: request.bridge_id,
		bridge_version: request.bridge_version,
		linked_native_project_id: None,
	};
	match state.metadata.upsert_external_project(&project, &files).await {
		Ok(()) => (StatusCode::CREATED, Json(project_view(project, files))).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn create_claim(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Path((provider, external_id)): Path<(String, String)>,
	Json(request): Json<ClaimRequest>,
) -> Response {
	if !user.allows("directory:manage") {
		return forbidden();
	}
	if request.claimant_ref.trim().is_empty() || request.challenge_ref.trim().is_empty() {
		return (StatusCode::BAD_REQUEST, "claimant_ref and challenge_ref are required").into_response();
	}
	match state.metadata.external_project(&provider, &external_id).await {
		Ok(None) => return (StatusCode::NOT_FOUND, "no such external project").into_response(),
		Err(error) => return storage_error(error),
		Ok(Some(_)) => {}
	}
	let recorded_at = now();
	let claim = ExternalClaimRow {
		id: new_id(),
		provider,
		external_project_id: external_id,
		claimant_ref: request.claimant_ref,
		challenge_ref: request.challenge_ref,
		state: "pending".to_string(),
		recorded_by: user.user_id,
		recorded_at,
		expires_at: recorded_at + 86_400,
		verified_at: None,
		reviewed_by: None,
	};
	let response = serde_json::json!({
		"id": claim.id,
		"state": claim.state,
		"challenge_ref": claim.challenge_ref,
		"expires_at": claim.expires_at,
	});
	match state.metadata.create_external_claim(&claim).await {
		Ok(()) => (StatusCode::ACCEPTED, Json(response)).into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
struct ClaimRequest {
	claimant_ref: String,
	challenge_ref: String,
}

#[derive(Deserialize)]
struct ClaimListQuery {
	state: Option<String>,
}

async fn list_claims(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	axum::extract::Query(query): axum::extract::Query<ClaimListQuery>,
) -> Response {
	if !user.allows("directory:manage") {
		return forbidden();
	}
	match state.metadata.external_claims(query.state.as_deref()).await {
		Ok(rows) => Json(rows.into_iter().map(claim_view).collect::<Vec<_>>()).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn review_claim(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Path(id): Path<String>,
	Json(request): Json<ReviewRequest>,
) -> Response {
	if !user.allows("directory:manage") {
		return forbidden();
	}
	if !matches!(request.state.as_str(), "approved" | "rejected") {
		return (StatusCode::BAD_REQUEST, "state must be approved or rejected").into_response();
	}
	match state
		.metadata
		.review_external_claim(
			&id,
			&request.state,
			&user.user_id,
			(request.state == "approved").then_some(now()),
			now(),
		)
		.await
	{
		Ok(true) => Json(serde_json::json!({ "id": id, "state": request.state })).into_response(),
		Ok(false) => (StatusCode::CONFLICT, "claim is missing or no longer pending").into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
struct ReviewRequest {
	state: String,
}

fn project_view(project: ExternalProjectRow, files: Vec<ExternalFileRow>) -> ProjectView {
	ProjectView {
		provider: project.provider,
		external_project_id: project.external_project_id,
		source_class: project.source_class,
		canonical_source_url: project.canonical_source_url,
		observed_profile: project.observed_profile,
		files: files.into_iter().map(file_view).collect(),
		observed_at: project.observed_at,
		last_synced_at: project.last_synced_at,
		source_state: project.source_state,
		bridge_id: project.bridge_id,
		bridge_version: project.bridge_version,
		linked_native_project_id: project.linked_native_project_id,
	}
}

fn file_view(file: ExternalFileRow) -> FileView {
	FileView {
		external_file_id: file.external_file_id,
		source_url: file.source_url,
		digest: file.digest.map(|digest| format!("sha256:{}", hex::encode(digest))),
		size: file.size,
		metadata: file.metadata,
		observed_at: file.observed_at,
		deleted_at: file.deleted_at,
	}
}

fn claim_view(claim: ExternalClaimRow) -> serde_json::Value {
	serde_json::json!({
		"id": claim.id,
		"provider": claim.provider,
		"external_project_id": claim.external_project_id,
		"claimant_ref": claim.claimant_ref,
		"challenge_ref": claim.challenge_ref,
		"state": claim.state,
		"recorded_by": claim.recorded_by,
		"recorded_at": claim.recorded_at,
		"expires_at": claim.expires_at,
		"verified_at": claim.verified_at,
		"reviewed_by": claim.reviewed_by,
	})
}

fn validate_project(provider: &str, external_id: &str, request: &UpsertRequest) -> Result<(), String> {
	for (name, value, max) in [
		("provider", provider, 128),
		("external_id", external_id, 256),
		("bridge_id", &request.bridge_id, 128),
		("bridge_version", &request.bridge_version, 64),
	] {
		if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
			return Err(format!("invalid {name}"));
		}
	}
	if request.source_class != "external-catalog" {
		return Err(
			"only external-catalog records are accepted until redistribution evidence and stored bytes exist".to_string(),
		);
	}
	if !matches!(
		request.source_state.as_str(),
		"active" | "deleted" | "unavailable" | "unknown"
	) {
		return Err("invalid source_state".to_string());
	}
	if !request.observed_profile.is_object() {
		return Err("observed_profile must be an object".to_string());
	}
	validate_https_url(&request.canonical_source_url, "canonical_source_url")?;
	Ok(())
}

fn parse_file(file: FileRequest) -> Result<ExternalFileRow, String> {
	if file.external_file_id.trim().is_empty() || file.external_file_id.len() > 256 {
		return Err("invalid external_file_id".to_string());
	}
	validate_https_url(&file.source_url, "source_url")?;
	if !file.metadata.is_object() {
		return Err("metadata must be an object".to_string());
	}
	if file.size.is_some_and(|size| size < 0) {
		return Err("size must not be negative".to_string());
	}
	let digest = file
		.digest
		.map(|digest| hex::decode(digest.trim()).map_err(|_| "digest must be hex".to_string()))
		.transpose()?;
	if digest.as_ref().is_some_and(|digest| digest.len() != 32) {
		return Err("digest must be a sha256 digest".to_string());
	}
	Ok(ExternalFileRow {
		external_file_id: file.external_file_id,
		source_url: file.source_url,
		digest,
		size: file.size,
		metadata: file.metadata,
		observed_at: file.observed_at,
		deleted_at: file.deleted_at,
	})
}

fn validate_https_url(value: &str, field: &str) -> Result<(), String> {
	let url = reqwest::Url::parse(value).map_err(|_| format!("{field} must be an absolute HTTPS URL"))?;
	if url.scheme() != "https" || url.host_str().is_none() {
		return Err(format!("{field} must be an absolute HTTPS URL"));
	}
	Ok(())
}

fn forbidden() -> Response {
	(StatusCode::FORBIDDEN, "the credential does not grant the required scope").into_response()
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "external project store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
