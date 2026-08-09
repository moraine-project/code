use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use moraine_codec::Value;
use moraine_model::Canonical;
use moraine_model::advisory::Advisory;
use moraine_model::delegation::Delegation;
use moraine_model::profile::ProfileRevision;
use serde::{Deserialize, Serialize};

use crate::registry::{OBJECT_CONTENT_TYPE, id_for, parse_hex_digest, storage_error};
use crate::routes::AppState;
use crate::store::StoredObject;

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/projects/{id}/profile", get(project_profile))
		.route("/v1/projects/{id}/releases/{hex}", get(release_view))
		.route("/v1/lookup", get(lookup))
		.route("/v1/objects/{hex}", get(object_bytes).head(object_head))
		.route("/v1/packs/{hex}", get(pack_view))
}

#[derive(Serialize)]
struct PackView {
	pack: String,
	payload: serde_json::Value,
}

async fn pack_view(State(state): State<AppState>, Path(hex_digest): Path<String>) -> Response {
	let Some(digest) = parse_hex_digest(&hex_digest) else {
		return (StatusCode::BAD_REQUEST, "invalid digest").into_response();
	};
	let Some(object) = (match state.metadata.object(&digest).await {
		Ok(object) => object,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::NOT_FOUND, "no such pack").into_response();
	};
	if object.kind != "modpack" {
		return (StatusCode::NOT_FOUND, "object is not a modpack manifest").into_response();
	}
	let payload = match payload_json(&object.payload) {
		Ok(payload) => payload,
		Err(error) => return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
	};
	Json(PackView {
		pack: id_for(&digest),
		payload,
	})
	.into_response()
}

pub(crate) fn payload_json(bytes: &[u8]) -> Result<serde_json::Value, String> {
	let value = moraine_codec::decode(bytes).map_err(|error| error.to_string())?;
	Ok(value_to_json(&value))
}

fn value_to_json(value: &Value) -> serde_json::Value {
	match value {
		Value::Integer(number) => serde_json::Value::from(*number),
		Value::Bytes(bytes) => serde_json::Value::from(hex::encode(bytes)),
		Value::Text(text) => serde_json::Value::from(text.clone()),
		Value::Bool(flag) => serde_json::Value::from(*flag),
		Value::Null => serde_json::Value::Null,
		Value::Array(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
		Value::Map(pairs) => {
			let mut object = serde_json::Map::new();
			for (key, item) in pairs {
				let name = key.as_text().map(str::to_string).unwrap_or_else(|| format!("{key:?}"));
				object.insert(name, value_to_json(item));
			}
			serde_json::Value::Object(object)
		}
	}
}

#[derive(Deserialize)]
struct LookupQuery {
	sha256: String,
}

#[derive(Serialize)]
struct LookupView {
	digest: String,
	matches: Vec<LookupMatch>,
}

#[derive(Serialize)]
struct LookupMatch {
	project_id: String,
	release: String,
	human_version: Option<String>,
	filename: Option<String>,
}

async fn lookup(State(state): State<AppState>, Query(query): Query<LookupQuery>) -> Response {
	let Some(digest) = parse_sha256(&query.sha256) else {
		return (StatusCode::BAD_REQUEST, "expected a sha256 digest").into_response();
	};
	let matches = match state.metadata.artifacts_for_digest(&digest).await {
		Ok(matches) => matches,
		Err(error) => return storage_error(error),
	};
	let mut views = Vec::with_capacity(matches.len());
	for entry in matches {
		let mut human_version = None;
		let mut filename = None;
		if let Ok(Some(object)) = state.metadata.object(&entry.release_digest).await
			&& let Ok(moraine_model::release::ReleaseObject::Release(release)) =
				moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload)
		{
			human_version = Some(release.human_version);
			filename = release
				.artifacts
				.iter()
				.find(|artifact| artifact.digest == digest)
				.map(|artifact| artifact.filename.clone());
		}
		views.push(LookupMatch {
			project_id: entry.project_id,
			release: id_for(&entry.release_digest),
			human_version,
			filename,
		});
	}
	Json(LookupView {
		digest: format!("sha256:{}", hex::encode(digest)),
		matches: views,
	})
	.into_response()
}

fn parse_sha256(value: &str) -> Option<[u8; 32]> {
	let hex = value.strip_prefix("sha256:").unwrap_or(value);
	hex::decode(hex).ok()?.try_into().ok()
}

#[derive(Serialize)]
struct ProfileView {
	project_id: String,
	display_name: String,
	summary: String,
	description: String,
	categories: Vec<String>,
	tags: Vec<String>,
	links: Vec<LinkView>,
	communities: Vec<LinkView>,
	revision: String,
}

#[derive(Serialize)]
struct LinkView {
	kind: String,
	url: String,
}

async fn project_profile(State(state): State<AppState>, Path(id): Path<String>) -> Response {
	let project = match state.metadata.project(&id).await {
		Ok(Some(project)) => project,
		Ok(None) => return (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => return storage_error(error),
	};
	let Some(revision_digest) = project.profile_digest else {
		return (StatusCode::NOT_FOUND, "no profile published").into_response();
	};
	let Some(object) = (match state.metadata.object(&revision_digest).await {
		Ok(object) => object,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "profile object is missing").into_response();
	};
	let Ok(profile) = ProfileRevision::from_canonical_bytes(&object.payload) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "stored profile does not decode").into_response();
	};
	let view = ProfileView {
		project_id: profile.project_id,
		display_name: profile.display_name,
		summary: profile.summary,
		description: profile.description,
		categories: profile.categories,
		tags: profile.tags,
		links: profile
			.links
			.into_iter()
			.map(|link| LinkView {
				kind: link.kind,
				url: link.url,
			})
			.collect(),
		communities: profile
			.communities
			.into_iter()
			.map(|link| LinkView {
				kind: link.kind,
				url: link.url,
			})
			.collect(),
		revision: id_for(&revision_digest),
	};
	Json(view).into_response()
}

#[derive(Serialize)]
struct ReleaseView {
	project_id: String,
	human_version: String,
	channel: String,
	kind: String,
	declared_time: i64,
	license_expression: Option<String>,
	artifacts: Vec<ArtifactView>,
	compatibility: Vec<CompatibilityView>,
	dependencies: Vec<DependencyView>,
	rights: Option<RightsView>,
	withdrawal: Option<WithdrawalView>,
	advisories: Vec<crate::advisories::AdvisoryView>,
}

#[derive(Serialize)]
struct WithdrawalView {
	reason: String,
	note: Option<String>,
	declared_time: i64,
}

#[derive(Serialize)]
struct ArtifactView {
	digest: String,
	size: u64,
	media_type: String,
	filename: String,
	is_primary: bool,
}

#[derive(Serialize)]
struct CompatibilityView {
	scheme: String,
	values: Vec<String>,
	loader_id: Option<String>,
	side: String,
}

#[derive(Serialize)]
struct DependencyView {
	target_kind: String,
	target_id: String,
	kind: String,
}

#[derive(Serialize)]
struct RightsView {
	redistribution: String,
	modpack_inclusion: String,
	mirroring: String,
	attribution_required: bool,
}

async fn release_view(State(state): State<AppState>, Path((id, hex_digest)): Path<(String, String)>) -> Response {
	let Some(digest) = parse_hex_digest(&hex_digest) else {
		return (StatusCode::BAD_REQUEST, "invalid digest").into_response();
	};
	let Some(object) = (match state.metadata.object(&digest).await {
		Ok(object) => object,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::NOT_FOUND, "no such release").into_response();
	};
	let release = match moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload) {
		Ok(moraine_model::release::ReleaseObject::Release(release)) => release,
		Ok(_) => return (StatusCode::NOT_FOUND, "object is not a release").into_response(),
		Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "stored release does not decode").into_response(),
	};
	if release.project_id != id {
		return (StatusCode::NOT_FOUND, "release does not belong to this project").into_response();
	}
	let release_id = id_for(&digest);
	let withdrawal = match state.metadata.withdrawal(&id, &release_id).await {
		Ok(withdrawal) => withdrawal,
		Err(error) => return storage_error(error),
	};
	let mut advisories = Vec::new();
	let mut seen = std::collections::HashSet::new();
	for artifact in &release.artifacts {
		let rows = match state.metadata.advisories_for_digest(&artifact.digest).await {
			Ok(rows) => rows,
			Err(error) => return storage_error(error),
		};
		for row in rows {
			if seen.insert(row.digest.clone()) {
				advisories.push(crate::advisories::advisory_view(row));
			}
		}
	}
	let view = ReleaseView {
		project_id: release.project_id,
		human_version: release.human_version,
		channel: release.channel,
		kind: release.kind,
		declared_time: release.declared_time,
		license_expression: release.license_expression,
		artifacts: release
			.artifacts
			.into_iter()
			.map(|artifact| ArtifactView {
				digest: format!("sha256:{}", hex::encode(artifact.digest)),
				size: artifact.size,
				media_type: artifact.media_type,
				filename: artifact.filename,
				is_primary: artifact.is_primary,
			})
			.collect(),
		compatibility: release
			.compatibility
			.into_iter()
			.map(|entry| CompatibilityView {
				scheme: entry.game_version_predicate.scheme,
				values: entry.game_version_predicate.values,
				loader_id: entry.loader_id,
				side: entry.side.as_str().to_string(),
			})
			.collect(),
		dependencies: release
			.dependencies
			.into_iter()
			.map(|dependency| DependencyView {
				target_kind: dependency.target_kind.as_str().to_string(),
				target_id: dependency.target_id,
				kind: dependency.kind.as_str().to_string(),
			})
			.collect(),
		rights: release.rights.map(|rights| RightsView {
			redistribution: rights.redistribution.as_str().to_string(),
			modpack_inclusion: rights.modpack_inclusion.as_str().to_string(),
			mirroring: rights.mirroring.as_str().to_string(),
			attribution_required: rights.attribution_required,
		}),
		withdrawal: withdrawal.map(|withdrawal| WithdrawalView {
			reason: withdrawal.reason,
			note: withdrawal.note,
			declared_time: withdrawal.declared_time,
		}),
		advisories,
	};
	Json(view).into_response()
}

async fn object_bytes(
	State(state): State<AppState>,
	Path(hex_digest): Path<String>,
	headers: HeaderMap,
	method: Method,
) -> Response {
	object_response(&state, &hex_digest, &headers, method == Method::HEAD).await
}

async fn object_head(State(state): State<AppState>, Path(hex_digest): Path<String>, headers: HeaderMap) -> Response {
	object_response(&state, &hex_digest, &headers, true).await
}

async fn object_response(state: &AppState, hex_digest: &str, headers: &HeaderMap, head: bool) -> Response {
	let Some(digest) = parse_hex_digest(hex_digest) else {
		return (StatusCode::BAD_REQUEST, "invalid digest").into_response();
	};
	match state.metadata.object(&digest).await {
		Ok(Some(object)) => serve_bytes(object.wire, headers, head),
		Ok(None) => (StatusCode::NOT_FOUND, "no such object").into_response(),
		Err(error) => storage_error(error),
	}
}

fn serve_bytes(bytes: Vec<u8>, headers: &HeaderMap, head: bool) -> Response {
	let length = bytes.len() as u64;
	let requested = headers
		.get(header::RANGE)
		.and_then(|value| value.to_str().ok())
		.and_then(|value| crate::routes::parse_range(value, length));
	let (status, start, end) = match requested {
		Some(Ok((start, end))) => (StatusCode::PARTIAL_CONTENT, start, end),
		Some(Err(())) => {
			let mut response = (StatusCode::RANGE_NOT_SATISFIABLE, Body::empty()).into_response();
			response.headers_mut().insert(
				header::CONTENT_RANGE,
				format!("bytes */{length}").parse().expect("valid header"),
			);
			return response;
		}
		None => (StatusCode::OK, 0, length.saturating_sub(1)),
	};
	let content_length = if length == 0 { 0 } else { end - start + 1 };
	let body = if head {
		Body::empty()
	} else {
		Body::from(bytes[start as usize..=end as usize].to_vec())
	};
	let mut response = Response::new(body);
	*response.status_mut() = status;
	let response_headers = response.headers_mut();
	response_headers.insert(header::CONTENT_TYPE, OBJECT_CONTENT_TYPE.parse().expect("valid header"));
	response_headers.insert(header::ACCEPT_RANGES, "bytes".parse().expect("valid header"));
	response_headers.insert(
		header::CACHE_CONTROL,
		"public, max-age=31536000, immutable".parse().expect("valid header"),
	);
	response_headers.insert(
		header::CONTENT_LENGTH,
		content_length.to_string().parse().expect("valid header"),
	);
	if status == StatusCode::PARTIAL_CONTENT {
		response_headers.insert(
			header::CONTENT_RANGE,
			format!("bytes {start}-{end}/{length}").parse().expect("valid header"),
		);
	}
	response
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ReleaseSummary {
	pub channel: String,
	pub game_id: String,
	pub loaders: Vec<String>,
}

pub(crate) fn release_matches_game_version(
	object: &StoredObject,
	version: &str,
	scheme: moraine_model::version::OrderingScheme,
) -> bool {
	use moraine_model::Canonical;
	let Ok(moraine_model::release::ReleaseObject::Release(release)) =
		moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload)
	else {
		return false;
	};
	let catalog = moraine_model::version::VersionCatalog::new(scheme, Vec::new());
	release.compatibility.iter().any(|entry| {
		catalog.evaluate(&entry.game_version_predicate, version) == moraine_model::compatibility::PredicateResult::Satisfied
	})
}

pub(crate) fn summarize_release(object: &StoredObject) -> Option<ReleaseSummary> {
	let release = match moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload) {
		Ok(moraine_model::release::ReleaseObject::Release(release)) => release,
		_ => return None,
	};
	let mut loaders: Vec<String> = release
		.compatibility
		.iter()
		.filter_map(|entry| entry.loader_id.clone())
		.collect();
	loaders.sort();
	loaders.dedup();
	Some(ReleaseSummary {
		channel: release.channel,
		game_id: release.game_id,
		loaders,
	})
}

pub(crate) fn describe_stored(object: &StoredObject) -> Option<String> {
	match object.kind.as_str() {
		"release" => match moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload) {
			Ok(moraine_model::release::ReleaseObject::Release(release)) => {
				Some(format!("{} ({})", release.human_version, release.channel))
			}
			Ok(moraine_model::release::ReleaseObject::Withdrawal(withdrawal)) => {
				Some(format!("withdrawn: {}", withdrawal.reason))
			}
			_ => None,
		},
		"profile" => ProfileRevision::from_canonical_bytes(&object.payload)
			.ok()
			.map(|profile| profile.display_name),
		"advisory" => Advisory::from_canonical_bytes(&object.payload)
			.ok()
			.map(|advisory| format!("{} {}", advisory.severity.as_str(), advisory.category.as_str())),
		"delegation" => Delegation::from_canonical_bytes(&object.payload)
			.ok()
			.map(|delegation| delegation.purpose().as_str().to_string()),
		_ => None,
	}
}
