use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::TryStreamExt;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::{ReaderStream, StreamReader};

use crate::blob::{BlobError, BlobStore};
use crate::capability::Capability;
use crate::store::MetadataStore;

#[derive(Clone)]
pub struct AppState {
	pub store: Arc<BlobStore>,
	pub metadata: Arc<MetadataStore>,
	pub capability: Arc<Capability>,
}

pub fn router(state: AppState) -> Router {
	Router::new()
		.route("/.well-known/mod-registry", get(well_known))
		.route("/healthz", get(|| async { "ok" }))
		.route("/readyz", get(ready))
		.route("/v1/blobs", post(blob_upload))
		.route("/v1/blobs/sha256/{digest}", get(blob_get).head(blob_head))
		.merge(crate::registry::routes())
		.merge(crate::auth::routes())
		.merge(crate::review::routes())
		.with_state(state)
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

async fn blob_upload(State(state): State<AppState>, body: Body) -> Response {
	let stream = body
		.into_data_stream()
		.map_err(|error| std::io::Error::other(error.to_string()));
	let reader = StreamReader::new(stream);
	let staged = match state.store.put_staged(reader, state.capability.max_artifact_bytes).await {
		Ok(staged) => staged,
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
	let Some(mut file) = (match state.store.open(&digest).await {
		Ok(file) => file,
		Err(error) => {
			tracing::error!(%error, "blob open failed");
			return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
		}
	}) else {
		return (StatusCode::NOT_FOUND, "no such blob").into_response();
	};
	let Ok(metadata) = file.metadata().await else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
	};
	let length = metadata.len();

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

	let content_length = if length == 0 { 0 } else { end - start + 1 };
	let body = if head {
		Body::empty()
	} else if status == StatusCode::PARTIAL_CONTENT {
		if let Err(error) = file.seek(std::io::SeekFrom::Start(start)).await {
			tracing::error!(%error, "blob seek failed");
			return (StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response();
		}
		Body::from_stream(ReaderStream::new(file.take(content_length)))
	} else {
		Body::from_stream(ReaderStream::new(file))
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

fn parse_digest(value: &str) -> Option<[u8; 32]> {
	let bytes = hex::decode(value).ok()?;
	bytes.try_into().ok()
}

fn parse_range(value: &str, length: u64) -> Option<Result<(u64, u64), ()>> {
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
mod tests {
	use axum::body::to_bytes;
	use http_body_util::BodyExt;
	use tower::ServiceExt;

	use super::*;

	async fn test_app() -> (Router, tempfile::TempDir) {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = Arc::new(BlobStore::new(directory.path()).await.expect("store"));
		let metadata = Arc::new(
			MetadataStore::open(directory.path().join("metadata.sqlite"))
				.await
				.expect("metadata"),
		);
		let config = crate::config::Config {
			bind: "127.0.0.1:0".parse().expect("addr"),
			data_dir: directory.path().to_path_buf(),
			max_artifact_bytes: 1024,
			max_feed_page_entries: 100,
			publishing: crate::config::Publishing::Review,
		};
		let state = AppState {
			store,
			metadata,
			capability: Arc::new(Capability::discover(&config)),
		};
		(router(state), directory)
	}

	#[tokio::test]
	async fn serves_capability_and_health() {
		let (app, _directory) = test_app().await;
		let response = app
			.clone()
			.oneshot(
				axum::http::Request::get("/.well-known/mod-registry")
					.body(Body::empty())
					.expect("request"),
			)
			.await
			.expect("response");
		assert_eq!(response.status(), StatusCode::OK);
		let body = to_bytes(response.into_body(), 16 * 1024).await.expect("body");
		let capability: serde_json::Value = serde_json::from_slice(&body).expect("json");
		assert_eq!(capability["protocol_versions"][0], 1);

		let health = app
			.oneshot(axum::http::Request::get("/healthz").body(Body::empty()).expect("request"))
			.await
			.expect("response");
		assert_eq!(health.status(), StatusCode::OK);
	}

	#[tokio::test]
	async fn uploads_then_serves_round_trip() {
		let (app, _directory) = test_app().await;
		let upload = axum::http::Request::post("/v1/blobs")
			.body(Body::from("artifact"))
			.expect("request");
		let response = app.clone().oneshot(upload).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);
		let body = to_bytes(response.into_body(), 16 * 1024).await.expect("body");
		let receipt: serde_json::Value = serde_json::from_slice(&body).expect("json");
		let digest = receipt["digest"].as_str().expect("digest");
		let hex_digest = digest.strip_prefix("sha256:").expect("prefix");
		let path = format!("/v1/blobs/sha256/{hex_digest}");

		let served = axum::http::Request::get(&path).body(Body::empty()).expect("request");
		let response = app.oneshot(served).await.expect("response");
		assert_eq!(response.status(), StatusCode::OK);
		let body = response.into_body().collect().await.expect("collect").to_bytes();
		assert_eq!(body.as_ref(), b"artifact");
	}

	#[tokio::test]
	async fn serves_blob_with_ranges() {
		let (app, directory) = test_app().await;
		let store = BlobStore::new(directory.path()).await.expect("store");
		let staged = store.put_staged(b"0123456789".as_slice(), 1024).await.expect("stage");
		let digest = store.commit(staged).await.expect("commit");
		let path = format!("/v1/blobs/sha256/{}", hex::encode(digest));

		let full = app
			.clone()
			.oneshot(axum::http::Request::get(&path).body(Body::empty()).expect("request"))
			.await
			.expect("response");
		assert_eq!(full.status(), StatusCode::OK);
		let body = full.into_body().collect().await.expect("collect").to_bytes();
		assert_eq!(body.as_ref(), b"0123456789");

		let range = axum::http::Request::get(&path)
			.header(header::RANGE, "bytes=2-5")
			.body(Body::empty())
			.expect("request");
		let partial = app.clone().oneshot(range).await.expect("response");
		assert_eq!(partial.status(), StatusCode::PARTIAL_CONTENT);
		assert_eq!(partial.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
		let body = partial.into_body().collect().await.expect("collect").to_bytes();
		assert_eq!(body.as_ref(), b"2345");

		let missing =
			axum::http::Request::get("/v1/blobs/sha256/0000000000000000000000000000000000000000000000000000000000000000")
				.body(Body::empty())
				.expect("request");
		let response = app.oneshot(missing).await.expect("response");
		assert_eq!(response.status(), StatusCode::NOT_FOUND);
	}
}
