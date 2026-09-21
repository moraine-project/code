use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;

use crate::auth::AuthenticatedUser;
use crate::db::MetadataStore;
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

#[derive(Debug, Clone)]
pub struct ExternalProjectRow {
	pub provider: String,
	pub external_project_id: String,
	pub source_class: String,
	pub canonical_source_url: String,
	pub observed_profile: Value,
	pub observed_at: i64,
	pub last_synced_at: i64,
	pub source_state: String,
	pub bridge_id: String,
	pub bridge_version: String,
	pub linked_native_project_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExternalFileRow {
	pub external_file_id: String,
	pub source_url: String,
	pub digest: Option<Vec<u8>>,
	pub size: Option<i64>,
	pub metadata: Value,
	pub observed_at: i64,
	pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ExternalClaimRow {
	pub id: String,
	pub provider: String,
	pub external_project_id: String,
	pub claimant_ref: String,
	pub challenge_ref: String,
	pub state: String,
	pub recorded_by: String,
	pub recorded_at: i64,
	pub expires_at: i64,
	pub verified_at: Option<i64>,
	pub reviewed_by: Option<String>,
}

impl MetadataStore {
	pub async fn upsert_external_project(
		&self,
		project: &ExternalProjectRow,
		files: &[ExternalFileRow],
	) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		sqlx::query(
			"INSERT INTO external_projects
			 (provider, external_project_id, source_class, canonical_source_url, observed_profile,
			  observed_at, last_synced_at, source_state, bridge_id, bridge_version, linked_native_project_id)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
			 ON CONFLICT(provider, external_project_id) DO UPDATE SET
			 source_class = $3, canonical_source_url = $4, observed_profile = $5, observed_at = $6,
			 last_synced_at = $7, source_state = $8, bridge_id = $9, bridge_version = $10,
			 linked_native_project_id = COALESCE($11, external_projects.linked_native_project_id)",
		)
		.bind(&project.provider)
		.bind(&project.external_project_id)
		.bind(&project.source_class)
		.bind(&project.canonical_source_url)
		.bind(project.observed_profile.to_string())
		.bind(project.observed_at)
		.bind(project.last_synced_at)
		.bind(&project.source_state)
		.bind(&project.bridge_id)
		.bind(&project.bridge_version)
		.bind(None::<String>)
		.execute(&mut *transaction)
		.await?;
		sqlx::query("DELETE FROM external_files WHERE provider = $1 AND external_project_id = $2")
			.bind(&project.provider)
			.bind(&project.external_project_id)
			.execute(&mut *transaction)
			.await?;
		for file in files {
			sqlx::query(
				"INSERT INTO external_files
				 (provider, external_file_id, external_project_id, source_url, digest, size, metadata_json, observed_at, deleted_at)
				 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
			)
			.bind(&project.provider)
			.bind(&file.external_file_id)
			.bind(&project.external_project_id)
			.bind(&file.source_url)
			.bind(&file.digest)
			.bind(file.size)
			.bind(file.metadata.to_string())
			.bind(file.observed_at)
			.bind(file.deleted_at)
			.execute(&mut *transaction)
			.await?;
		}
		transaction.commit().await
	}

	pub async fn external_project(
		&self,
		provider: &str,
		external_project_id: &str,
	) -> Result<Option<(ExternalProjectRow, Vec<ExternalFileRow>)>, sqlx::Error> {
		let project = sqlx::query(
			"SELECT provider, external_project_id, source_class, canonical_source_url, observed_profile,
			 observed_at, last_synced_at, source_state, bridge_id, bridge_version, linked_native_project_id
			 FROM external_projects WHERE provider = $1 AND external_project_id = $2",
		)
		.bind(provider)
		.bind(external_project_id)
		.fetch_optional(&self.pool)
		.await?;
		let Some(row) = project else { return Ok(None) };
		let project = ExternalProjectRow {
			provider: row.get("provider"),
			external_project_id: row.get("external_project_id"),
			source_class: row.get("source_class"),
			canonical_source_url: row.get("canonical_source_url"),
			observed_profile: parse_json(row.get("observed_profile"))?,
			observed_at: row.get("observed_at"),
			last_synced_at: row.get("last_synced_at"),
			source_state: row.get("source_state"),
			bridge_id: row.get("bridge_id"),
			bridge_version: row.get("bridge_version"),
			linked_native_project_id: row.get("linked_native_project_id"),
		};
		let rows = sqlx::query(
			"SELECT external_file_id, source_url, digest, size, metadata_json, observed_at, deleted_at
			 FROM external_files WHERE provider = $1 AND external_project_id = $2 ORDER BY external_file_id",
		)
		.bind(provider)
		.bind(external_project_id)
		.fetch_all(&self.pool)
		.await?;
		let files = rows
			.into_iter()
			.map(|row| {
				Ok(ExternalFileRow {
					external_file_id: row.get("external_file_id"),
					source_url: row.get("source_url"),
					digest: row.get("digest"),
					size: row.get("size"),
					metadata: parse_json(row.get("metadata_json"))?,
					observed_at: row.get("observed_at"),
					deleted_at: row.get("deleted_at"),
				})
			})
			.collect::<Result<Vec<_>, sqlx::Error>>()?;
		Ok(Some((project, files)))
	}

	pub async fn create_external_claim(&self, claim: &ExternalClaimRow) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO external_project_claims
			 (id, provider, external_project_id, claimant_ref, challenge_ref, state, recorded_by, recorded_at, expires_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
		)
		.bind(&claim.id)
		.bind(&claim.provider)
		.bind(&claim.external_project_id)
		.bind(&claim.claimant_ref)
		.bind(&claim.challenge_ref)
		.bind(&claim.state)
		.bind(&claim.recorded_by)
		.bind(claim.recorded_at)
		.bind(claim.expires_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn external_claims(&self, state: Option<&str>) -> Result<Vec<ExternalClaimRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, provider, external_project_id, claimant_ref, challenge_ref, state, recorded_by,
			 recorded_at, expires_at, verified_at, reviewed_by FROM external_project_claims
			 WHERE ($1 IS NULL OR state = $1) ORDER BY recorded_at DESC, id ASC",
		)
		.bind(state)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(claim_row).collect())
	}

	pub async fn review_external_claim(
		&self,
		id: &str,
		state: &str,
		reviewed_by: &str,
		verified_at: Option<i64>,
		now: i64,
	) -> Result<bool, sqlx::Error> {
		let result = sqlx::query(
			"UPDATE external_project_claims SET state = $2, reviewed_by = $3, verified_at = $4
			 WHERE id = $1 AND state = 'pending' AND expires_at > $5",
		)
		.bind(id)
		.bind(state)
		.bind(reviewed_by)
		.bind(verified_at)
		.bind(now)
		.execute(&self.pool)
		.await?;
		Ok(result.rows_affected() == 1)
	}
}

fn parse_json(value: String) -> Result<Value, sqlx::Error> {
	serde_json::from_str(&value).map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

fn claim_row(row: sqlx::any::AnyRow) -> ExternalClaimRow {
	ExternalClaimRow {
		id: row.get("id"),
		provider: row.get("provider"),
		external_project_id: row.get("external_project_id"),
		claimant_ref: row.get("claimant_ref"),
		challenge_ref: row.get("challenge_ref"),
		state: row.get("state"),
		recorded_by: row.get("recorded_by"),
		recorded_at: row.get("recorded_at"),
		expires_at: row.get("expires_at"),
		verified_at: row.get("verified_at"),
		reviewed_by: row.get("reviewed_by"),
	}
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
		.map(|elapsed| elapsed.as_secs() as i64)
		.unwrap_or(0)
}
