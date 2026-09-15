use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::TryStreamExt;
use serde::Serialize;
use tokio_util::io::StreamReader;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;

use crate::auth::AuthenticatedUser;
use crate::blob::{BlobError, BlobStore};
use crate::capability::Capability;
use crate::db::MetadataStore;

const MAX_REQUEST_BODY_BYTES: usize = 256 * 1024;
const REQUEST_TIMEOUT_SECONDS: u64 = 30;

static UPLOAD_LOCKS: std::sync::OnceLock<crate::blob::UploadLocks> = std::sync::OnceLock::new();

#[derive(Clone)]
pub struct AppState {
	pub store: Arc<BlobStore>,
	pub metadata: Arc<MetadataStore>,
	pub capability: Arc<Capability>,
	pub login_limiter: Arc<crate::auth::LoginLimiter>,
	pub metrics: Arc<crate::ops::metrics::Metrics>,
	pub rate_limiter: Arc<crate::auth::ratelimit::RateLimiter>,
	pub web_dir: Option<Arc<std::path::PathBuf>>,
}

pub fn router(state: AppState) -> Router {
	let web_dir = state.web_dir.clone();
	let web_origins = state.capability.web_origins.clone();
	let app = Router::new()
		.route("/.well-known/mod-registry", get(well_known))
		.route("/healthz", get(|| async { "ok" }))
		.route("/readyz", get(ready))
		.route("/v1/blobs", post(blob_upload))
		.route("/v1/blobs/sha256/{digest}", get(blob_get).head(blob_head))
		.merge(crate::registry::routes())
		.merge(crate::auth::routes())
		.merge(crate::registry::review::routes())
		.merge(crate::federation::routes())
		.merge(crate::registry::search::routes())
		.merge(crate::auth::orgs::routes())
		.merge(crate::registry::advisories::routes())
		.merge(crate::registry::attestations::routes())
		.merge(crate::federation::mirrors::routes())
		.merge(crate::federation::notifications::routes())
		.merge(crate::federation::webhooks::routes())
		.merge(crate::registry::definitions::routes())
		.merge(crate::ops::metrics::routes())
		.layer(axum::middleware::from_fn_with_state(
			state.clone(),
			crate::ops::metrics::track,
		))
		.layer(axum::middleware::from_fn_with_state(
			state.clone(),
			crate::auth::ratelimit::limit,
		))
		.with_state(state);
	let app = match web_dir {
		Some(directory) => {
			let index = directory.join("index.html");
			app.fallback_service(ServeDir::new(&*directory).fallback(ServeFile::new(index)))
		}
		None => app,
	};
	app.layer(cors(&web_origins))
		.layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
		.layer(TimeoutLayer::with_status_code(
			StatusCode::REQUEST_TIMEOUT,
			Duration::from_secs(REQUEST_TIMEOUT_SECONDS),
		))
		.layer(SetResponseHeaderLayer::if_not_present(
			axum::http::header::CONTENT_SECURITY_POLICY,
			HeaderValue::from_static("frame-ancestors 'none'; base-uri 'self'; object-src 'none'"),
		))
		.layer(SetResponseHeaderLayer::if_not_present(
			axum::http::header::X_CONTENT_TYPE_OPTIONS,
			HeaderValue::from_static("nosniff"),
		))
		.layer(SetResponseHeaderLayer::if_not_present(
			axum::http::header::REFERRER_POLICY,
			HeaderValue::from_static("no-referrer"),
		))
}

fn cors(origins: &[String]) -> CorsLayer {
	if origins.is_empty() {
		return CorsLayer::new()
			.allow_origin(Any)
			.allow_methods([Method::GET, Method::HEAD])
			.allow_headers(Any)
			.max_age(Duration::from_secs(3600));
	}
	let allowed: Vec<HeaderValue> = origins.iter().filter_map(|origin| origin.parse().ok()).collect();
	CorsLayer::new()
		.allow_origin(AllowOrigin::list(allowed))
		.allow_credentials(true)
		.allow_methods([
			Method::GET,
			Method::HEAD,
			Method::POST,
			Method::PUT,
			Method::PATCH,
			Method::DELETE,
		])
		.allow_headers([
			header::CONTENT_TYPE,
			header::ACCEPT,
			header::RANGE,
			header::IF_NONE_MATCH,
			axum::http::HeaderName::from_static("x-csrf-token"),
		])
		.max_age(Duration::from_secs(3600))
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

#[derive(Serialize)]
struct UploadReceipt {
	digest: String,
	size: u64,
}

async fn blob_upload(State(state): State<AppState>, user: AuthenticatedUser, body: Body) -> Response {
	if !user.allows("artifacts:write") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	if let Some(message) = crate::registry::sanctions::publishing_block(&state, &user.user_id).await {
		return (StatusCode::FORBIDDEN, message).into_response();
	}
	if let Some(response) = crate::auth::verified_or_error(&state, &user.user_id).await {
		return response;
	}
	let locks = UPLOAD_LOCKS.get_or_init(crate::blob::UploadLocks::new);
	let account_lock = locks.get(&user.user_id);
	let _guard = account_lock.lock().await;
	let used = match state.metadata.upload_bytes_for(&user.user_id).await {
		Ok(used) => used,
		Err(error) => {
			tracing::error!(%error, "upload quota lookup failed");
			return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
		}
	};
	let remaining = state
		.capability
		.max_upload_bytes_per_account
		.saturating_sub(used.max(0) as u64);
	if remaining == 0 {
		return (StatusCode::FORBIDDEN, "the account has reached its upload quota").into_response();
	}
	let limit = state.capability.max_artifact_bytes.min(remaining);
	let stream = body
		.into_data_stream()
		.map_err(|error| std::io::Error::other(error.to_string()));
	let reader = StreamReader::new(stream);
	let staged = match state.store.put_staged(reader, limit).await {
		Ok(staged) => staged,
		Err(BlobError::TooLarge { .. }) if limit < state.capability.max_artifact_bytes => {
			return (StatusCode::FORBIDDEN, "the account has reached its upload quota").into_response();
		}
		Err(BlobError::TooLarge { limit }) => {
			return (StatusCode::PAYLOAD_TOO_LARGE, format!("artifact exceeds {limit} bytes")).into_response();
		}
		Err(BlobError::Io(error)) => {
			tracing::error!(%error, "staged upload failed");
			return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
		}
	};
	let size = staged.size;
	let digest = match state.store.commit(staged).await {
		Ok(digest) => digest,
		Err(error) => {
			tracing::error!(%error, "blob commit failed");
			return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
		}
	};
	if let Err(error) = state
		.metadata
		.record_upload(&digest, &user.user_id, size as i64, crate::auth::now())
		.await
	{
		tracing::error!(%error, "upload record failed");
		return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
	}
	let hex_digest = hex::encode(digest);
	let mut response = (
		StatusCode::CREATED,
		Json(UploadReceipt {
			digest: format!("sha256:{hex_digest}"),
			size,
		}),
	)
		.into_response();
	response.headers_mut().insert(
		header::LOCATION,
		format!("/v1/blobs/sha256/{hex_digest}").parse().expect("valid header"),
	);
	response
}

async fn blob_get(
	State(state): State<AppState>,
	Path(digest): Path<String>,
	headers: HeaderMap,
	method: Method,
) -> Response {
	serve_blob(&state, &digest, &headers, method == Method::HEAD).await
}

async fn blob_head(State(state): State<AppState>, Path(digest): Path<String>, headers: HeaderMap) -> Response {
	serve_blob(&state, &digest, &headers, true).await
}

async fn serve_blob(state: &AppState, digest_hex: &str, headers: &HeaderMap, head: bool) -> Response {
	let Some(digest) = parse_digest(digest_hex) else {
		return (StatusCode::BAD_REQUEST, "invalid digest").into_response();
	};
	match state.metadata.blob_is_referenced(&digest).await {
		Ok(true) => {}
		Ok(false) => return (StatusCode::NOT_FOUND, "no such blob").into_response(),
		Err(error) => {
			tracing::error!(%error, "blob reference lookup failed");
			return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
		}
	}
	let Some(length) = (match state.store.size(&digest).await {
		Ok(size) => size,
		Err(error) => {
			tracing::error!(%error, "blob size lookup failed");
			return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
		}
	}) else {
		state.metrics.record_blob_serve_failure();
		return (StatusCode::NOT_FOUND, "no such blob").into_response();
	};

	let requested = headers
		.get(header::RANGE)
		.and_then(|value| value.to_str().ok())
		.and_then(|value| parse_range(value, length));
	let (status, start, end) = match requested {
		Some(Ok((start, end))) => (StatusCode::PARTIAL_CONTENT, start, end),
		Some(Err(())) => {
			let mut response = (StatusCode::RANGE_NOT_SATISFIABLE, "range not satisfiable").into_response();
			response.headers_mut().insert(
				header::CONTENT_RANGE,
				format!("bytes */{length}").parse().expect("valid header"),
			);
			return response;
		}
		None => (StatusCode::OK, 0, length.saturating_sub(1)),
	};

	if !head {
		let metadata = state.metadata.clone();
		tokio::spawn(async move {
			let _ = metadata.record_download(&digest, unix_day()).await;
		});
	}

	let content_length = if length == 0 { 0 } else { end - start + 1 };
	let body = if head {
		Body::empty()
	} else {
		let range = (status == StatusCode::PARTIAL_CONTENT).then_some((start, end));
		match state.store.read(&digest, range).await {
			Ok(Some(stream)) => Body::from_stream(stream),
			Ok(None) => {
				state.metrics.record_blob_serve_failure();
				return (StatusCode::NOT_FOUND, "no such blob").into_response();
			}
			Err(error) => {
				tracing::error!(%error, "blob read failed");
				state.metrics.record_blob_serve_failure();
				return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
			}
		}
	};

	let mut response = Response::new(body);
	*response.status_mut() = status;
	let response_headers = response.headers_mut();
	response_headers.insert(
		header::CONTENT_TYPE,
		"application/octet-stream".parse().expect("valid header"),
	);
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

fn unix_day() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|elapsed| (elapsed.as_secs() / 86_400) as i64)
		.unwrap_or(0)
}

fn parse_digest(value: &str) -> Option<[u8; 32]> {
	let bytes = hex::decode(value).ok()?;
	bytes.try_into().ok()
}

pub(crate) fn parse_range(value: &str, length: u64) -> Option<Result<(u64, u64), ()>> {
	let spec = value.strip_prefix("bytes=")?;
	if spec.contains(',') {
		return None;
	}
	let (start, end) = spec.split_once('-')?;
	if start.is_empty() {
		let suffix: u64 = end.parse().ok()?;
		if suffix == 0 || length == 0 {
			return Some(Err(()));
		}
		return Some(Ok((length.saturating_sub(suffix), length - 1)));
	}
	let start: u64 = start.parse().ok()?;
	if start >= length {
		return Some(Err(()));
	}
	let end = if end.is_empty() {
		length - 1
	} else {
		end.parse::<u64>().ok()?.min(length - 1)
	};
	if end < start {
		return Some(Err(()));
	}
	Some(Ok((start, end)))
}

#[cfg(test)]
mod tests;
