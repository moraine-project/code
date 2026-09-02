use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_crypto::{ObjectKind, object_id};
use moraine_model::deny_list::{DenyList, DenyListEntry, DenyTarget};
use moraine_model::moderation::ScopeKind;
use moraine_model::signed::{SignedObject, TrustedKey, verify_envelope};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::db::{MetadataStore, StoredObject};
use crate::routes::AppState;

pub(crate) fn routes() -> Router<AppState> {
	Router::new().route("/v1/deny-lists", get(list_entries).post(publish_list))
}

#[derive(Debug, Clone)]
pub struct DenyEntryRow {
	pub issuer_id: String,
	pub entry: DenyListEntry,
	pub object_digest: Vec<u8>,
}

impl MetadataStore {
	pub async fn replace_deny_list(
		&self,
		issuer_id: &str,
		entries: &[DenyListEntry],
		object_digest: &[u8],
		issued_at: i64,
	) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		sqlx::query("DELETE FROM deny_list_entries WHERE issuer_id = $1")
			.bind(issuer_id)
			.execute(&mut *transaction)
			.await?;
		for entry in entries {
			sqlx::query(
				"INSERT INTO deny_list_entries (issuer_id, target_kind, target_id, reason_code, reason_taxonomy_version,
				 scope_kind, scope_id, valid_from, valid_until, object_digest, issued_at)
				 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
			)
			.bind(issuer_id)
			.bind(entry.target_kind.as_str())
			.bind(&entry.target_id)
			.bind(&entry.reason_code)
			.bind(i64::from(entry.reason_taxonomy_version))
			.bind(entry.scope_kind.as_str())
			.bind(&entry.scope_id)
			.bind(entry.valid_from)
			.bind(entry.valid_until)
			.bind(object_digest)
			.bind(issued_at)
			.execute(&mut *transaction)
			.await?;
		}
		transaction.commit().await?;
		Ok(())
	}

	pub async fn deny_entries_for(
		&self,
		target_kind: &str,
		target_id: &str,
		now: i64,
	) -> Result<Vec<DenyEntryRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT issuer_id, target_kind, target_id, reason_code, reason_taxonomy_version, scope_kind, scope_id,
			 valid_from, valid_until, object_digest FROM deny_list_entries
			 WHERE target_kind = $1 AND target_id = $2
			 AND (valid_from IS NULL OR valid_from <= $3) AND (valid_until IS NULL OR valid_until > $3)
			 ORDER BY issuer_id ASC",
		)
		.bind(target_kind)
		.bind(target_id)
		.bind(now)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().filter_map(deny_row).collect())
	}

	pub async fn deny_entries_for_projects(
		&self,
		project_ids: &[String],
		now: i64,
	) -> Result<std::collections::HashMap<String, Vec<DenyEntryRow>>, sqlx::Error> {
		let mut entries: std::collections::HashMap<String, Vec<DenyEntryRow>> = std::collections::HashMap::new();
		if project_ids.is_empty() {
			return Ok(entries);
		}
		let mut builder = crate::db::sql::SqlBuilder::new(
			"SELECT issuer_id, target_kind, target_id, reason_code, reason_taxonomy_version, scope_kind, scope_id,
			 valid_from, valid_until, object_digest FROM deny_list_entries
			 WHERE target_kind = 'project' AND (valid_from IS NULL OR valid_from <= ",
		);
		builder.push_bind(now);
		builder.push(") AND (valid_until IS NULL OR valid_until > ");
		builder.push_bind(now);
		builder.push(") AND target_id IN (");
		{
			let mut separated = builder.separated(", ");
			for project_id in project_ids {
				separated.push_bind(project_id);
			}
		}
		builder.push(") ORDER BY issuer_id ASC");
		for row in builder.into_query().fetch_all(&self.pool).await? {
			if let Some(entry) = deny_row(row) {
				entries.entry(entry.entry.target_id.clone()).or_default().push(entry);
			}
		}
		Ok(entries)
	}
}

fn deny_row(row: sqlx::any::AnyRow) -> Option<DenyEntryRow> {
	let target_kind: String = row.get("target_kind");
	let scope_kind: String = row.get("scope_kind");
	let taxonomy_version: i64 = row.get("reason_taxonomy_version");
	Some(DenyEntryRow {
		issuer_id: row.get("issuer_id"),
		entry: DenyListEntry {
			target_kind: DenyTarget::parse(&target_kind)?,
			target_id: row.get("target_id"),
			reason_code: row.get("reason_code"),
			reason_taxonomy_version: u32::try_from(taxonomy_version).ok()?,
			scope_kind: ScopeKind::parse(&scope_kind)?,
			scope_id: row.get("scope_id"),
			valid_from: row.get("valid_from"),
			valid_until: row.get("valid_until"),
		},
		object_digest: row.get("object_digest"),
	})
}

#[derive(Serialize)]
struct DenyEntryView {
	issuer_id: String,
	target_kind: String,
	target_id: String,
	reason_code: String,
	reason_taxonomy_version: u32,
	scope_kind: String,
	scope_id: String,
	valid_from: Option<i64>,
	valid_until: Option<i64>,
	deny_list: String,
}

fn view(row: DenyEntryRow) -> DenyEntryView {
	DenyEntryView {
		issuer_id: row.issuer_id,
		target_kind: row.entry.target_kind.as_str().to_string(),
		target_id: row.entry.target_id,
		reason_code: row.entry.reason_code,
		reason_taxonomy_version: row.entry.reason_taxonomy_version,
		scope_kind: row.entry.scope_kind.as_str().to_string(),
		scope_id: row.entry.scope_id,
		valid_from: row.entry.valid_from,
		valid_until: row.entry.valid_until,
		deny_list: format!("gd:sha256:{}", hex::encode(&row.object_digest)),
	}
}

async fn publish_list(State(state): State<AppState>, body: Bytes) -> Response {
	let signed = match SignedObject::<DenyList>::from_bytes(&body) {
		Ok(signed) => signed,
		Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	};
	let list = &signed.payload;
	let Some(public_key) = (match state.metadata.provider(&list.issuer_id).await {
		Ok(provider) => provider,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::CONFLICT, "issuer key is not pinned on this instance").into_response();
	};
	let trusted = match TrustedKey::new(&public_key) {
		Ok(trusted) => trusted,
		Err(error) => return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
	};
	if verify_envelope(&signed.envelope, &signed.signed_message(ObjectKind::DenyList), &[trusted], 1).is_err() {
		return (
			StatusCode::BAD_REQUEST,
			"deny list signature did not verify against the pinned key",
		)
			.into_response();
	}
	let digest = object_id(ObjectKind::DenyList, &signed.payload_bytes);
	let stored = StoredObject {
		digest: digest.to_vec(),
		kind: "deny-list".to_string(),
		payload: signed.payload_bytes.clone(),
		wire: body.to_vec(),
	};
	if let Err(error) = state.metadata.put_object(&stored).await {
		return storage_error(error);
	}
	if let Err(error) = state
		.metadata
		.replace_deny_list(&list.issuer_id, &list.entries, &digest, list.issued_at)
		.await
	{
		return storage_error(error);
	}
	(
		StatusCode::CREATED,
		Json(serde_json::json!({
			"deny_list": format!("gd:sha256:{}", hex::encode(digest)),
			"entries": list.entries.len(),
		})),
	)
		.into_response()
}

#[derive(Deserialize)]
struct ListQuery {
	project: Option<String>,
	digest: Option<String>,
}

async fn list_entries(State(state): State<AppState>, Query(query): Query<ListQuery>) -> Response {
	let (kind, id) = match (query.project, query.digest) {
		(Some(project), _) => (DenyTarget::Project, project),
		(None, Some(digest)) => (DenyTarget::ArtifactDigest, digest),
		(None, None) => return (StatusCode::BAD_REQUEST, "a project or digest is required").into_response(),
	};
	match state.metadata.deny_entries_for(kind.as_str(), &id, now()).await {
		Ok(rows) => Json(rows.into_iter().map(view).collect::<Vec<_>>()).into_response(),
		Err(error) => storage_error(error),
	}
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "deny list store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
