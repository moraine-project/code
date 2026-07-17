use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::routes::AppState;

pub struct Metrics {
	started: Instant,
	requests: AtomicU64,
	server_errors: AtomicU64,
}

impl Metrics {
	pub fn new() -> Self {
		Self {
			started: Instant::now(),
			requests: AtomicU64::new(0),
			server_errors: AtomicU64::new(0),
		}
	}

	fn observe(&self, status: u16) {
		self.requests.fetch_add(1, Ordering::Relaxed);
		if status >= 500 {
			self.server_errors.fetch_add(1, Ordering::Relaxed);
		}
	}
}

impl Default for Metrics {
	fn default() -> Self {
		Self::new()
	}
}

pub fn routes() -> Router<AppState> {
	Router::new().route("/metrics", get(render))
}

pub async fn track(State(state): State<AppState>, request: Request, next: Next) -> Response {
	let response = next.run(request).await;
	state.metrics.observe(response.status().as_u16());
	response
}

async fn render(State(state): State<AppState>) -> Response {
	let snapshot = match state.metadata.metrics_snapshot().await {
		Ok(snapshot) => snapshot,
		Err(error) => {
			tracing::error!(%error, "metrics snapshot failed");
			return crate::registry::storage_error(error);
		}
	};
	let mut body = String::new();
	for (name, value) in snapshot.lines() {
		body.push_str("# TYPE ");
		body.push_str(name);
		body.push_str(" gauge\n");
		body.push_str(name);
		body.push(' ');
		body.push_str(&value.to_string());
		body.push('\n');
	}
	for (name, value) in [
		("moraine_uptime_seconds", state.metrics.started.elapsed().as_secs()),
		("moraine_requests_total", state.metrics.requests.load(Ordering::Relaxed)),
		(
			"moraine_server_errors_total",
			state.metrics.server_errors.load(Ordering::Relaxed),
		),
	] {
		body.push_str("# TYPE ");
		body.push_str(name);
		body.push_str(" counter\n");
		body.push_str(name);
		body.push(' ');
		body.push_str(&value.to_string());
		body.push('\n');
	}
	let mut response = body.into_response();
	response.headers_mut().insert(
		header::CONTENT_TYPE,
		header::HeaderValue::from_static("text/plain; version=0.0.4"),
	);
	response
}

#[cfg(test)]
mod tests {
	use tower::ServiceExt;

	use crate::test_support::app;

	#[tokio::test]
	async fn reports_counts_and_process_totals() {
		let (application, _directory) = app().await;
		let request = axum::http::Request::get("/metrics")
			.body(axum::body::Body::empty())
			.expect("request");
		let response = application.oneshot(request).await.expect("response");
		assert_eq!(response.status(), axum::http::StatusCode::OK);
		let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.expect("body");
		let text = String::from_utf8(body.to_vec()).expect("utf8");
		for name in ["moraine_projects_total", "moraine_requests_total", "moraine_uptime_seconds"] {
			assert!(text.contains(name), "missing {name}");
		}
	}
}
