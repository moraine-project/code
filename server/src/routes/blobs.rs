use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use futures_util::TryStreamExt;
use serde::Serialize;
use tokio_util::io::StreamReader;

use crate::auth::AuthenticatedUser;
use crate::blob::BlobError;
use crate::routes::AppState;

static UPLOAD_LOCKS: std::sync::OnceLock<crate::blob::UploadLocks> = std::sync::OnceLock::new();

pub(super) fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/blobs", post(blob_upload))
		.route("/v1/blobs/sha256/{digest}", axum::routing::get(blob_get).head(blob_head))
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
