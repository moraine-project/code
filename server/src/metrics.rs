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
	signature_failures: AtomicU64,
	federation_network_failures: AtomicU64,
	federation_protocol_failures: AtomicU64,
	federation_signature_failures: AtomicU64,
	federation_storage_failures: AtomicU64,
	federation_rejections: AtomicU64,
}

impl Metrics {
	pub fn new() -> Self {
		Self {
			started: Instant::now(),
			requests: AtomicU64::new(0),
			server_errors: AtomicU64::new(0),
			signature_failures: AtomicU64::new(0),
			federation_network_failures: AtomicU64::new(0),
			federation_protocol_failures: AtomicU64::new(0),
			federation_signature_failures: AtomicU64::new(0),
			federation_storage_failures: AtomicU64::new(0),
			federation_rejections: AtomicU64::new(0),
		}
	}

	pub fn record_signature_failure(&self) {
		self.signature_failures.fetch_add(1, Ordering::Relaxed);
	}

	pub fn record_federation_failure(&self, error: &crate::federation::FederationError) {
		use crate::federation::FederationError;
		let counter = match error {
			FederationError::Http(_) => &self.federation_network_failures,
			FederationError::InvalidUrl(_) | FederationError::Decode(_) => &self.federation_protocol_failures,
			FederationError::Verify(_) => &self.federation_signature_failures,
			FederationError::Storage(_) => &self.federation_storage_failures,
			FederationError::Rejected(_) => &self.federation_rejections,
		};
		counter.fetch_add(1, Ordering::Relaxed);
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
		(
			"moraine_signature_failures_total",
			state.metrics.signature_failures.load(Ordering::Relaxed),
		),
		(
			"moraine_federation_network_failures_total",
			state.metrics.federation_network_failures.load(Ordering::Relaxed),
		),
		(
			"moraine_federation_protocol_failures_total",
			state.metrics.federation_protocol_failures.load(Ordering::Relaxed),
		),
		(
			"moraine_federation_signature_failures_total",
			state.metrics.federation_signature_failures.load(Ordering::Relaxed),
		),
		(
			"moraine_federation_storage_failures_total",
			state.metrics.federation_storage_failures.load(Ordering::Relaxed),
		),
		(
			"moraine_federation_rejections_total",
			state.metrics.federation_rejections.load(Ordering::Relaxed),
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
	let current = unix_now();
	for (name, since) in [
		("moraine_admission_oldest_seconds", snapshot.oldest_pending_submission),
		("moraine_webhook_backlog_oldest_seconds", snapshot.oldest_pending_delivery),
	] {
		let age = since.map(|since| (current - since).max(0)).unwrap_or(0);
		body.push_str("# TYPE ");
		body.push_str(name);
		body.push_str(" gauge\n");
		body.push_str(name);
		body.push(' ');
		body.push_str(&age.to_string());
		body.push('\n');
	}
	let mut response = body.into_response();
	response.headers_mut().insert(
		header::CONTENT_TYPE,
		header::HeaderValue::from_static("text/plain; version=0.0.4"),
	);
	response
}

fn unix_now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|elapsed| elapsed.as_secs() as i64)
		.unwrap_or(0)
}

#[cfg(test)]
mod tests {
	use std::sync::atomic::Ordering;

	use tower::ServiceExt;

	use super::*;
	use crate::test_support::app;

	#[test]
	fn classifies_federation_failures_by_cause() {
		use crate::federation::FederationError;

		let metrics = Metrics::new();
		for error in [
			FederationError::Http("x".to_string()),
			FederationError::Verify("x".to_string()),
			FederationError::InvalidUrl("x".to_string()),
			FederationError::Storage("x".to_string()),
			FederationError::Rejected("x".to_string()),
		] {
			metrics.record_federation_failure(&error);
		}

		assert_eq!(metrics.federation_network_failures.load(Ordering::Relaxed), 1);
		assert_eq!(metrics.federation_protocol_failures.load(Ordering::Relaxed), 1);
		assert_eq!(metrics.federation_signature_failures.load(Ordering::Relaxed), 1);
		assert_eq!(metrics.federation_storage_failures.load(Ordering::Relaxed), 1);
		assert_eq!(metrics.federation_rejections.load(Ordering::Relaxed), 1);
	}

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
		for name in [
			"moraine_projects_total",
			"moraine_requests_total",
			"moraine_uptime_seconds",
			"moraine_federation_protocol_failures_total",
			"moraine_federation_storage_failures_total",
			"moraine_federation_rejections_total",
			"moraine_admission_oldest_seconds",
			"moraine_webhook_backlog_oldest_seconds",
			"moraine_subscription_lag_entries",
		] {
			assert!(text.contains(name), "missing {name}");
		}
	}
}
