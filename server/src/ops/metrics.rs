use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::db::MetadataStore;
use crate::routes::AppState;

pub struct Metrics {
	started: Instant,
	requests: AtomicU64,
	request_micros: AtomicU64,
	server_errors: AtomicU64,
	sessions_created: AtomicU64,
	sessions_revoked: AtomicU64,
	api_keys_created: AtomicU64,
	api_keys_revoked: AtomicU64,
	staging_collected: AtomicU64,
	blobs_collected: AtomicU64,
	blob_serve_failures: AtomicU64,
	key_changes: AtomicU64,
	signature_failures: AtomicU64,
	federation_network_failures: AtomicU64,
	federation_protocol_failures: AtomicU64,
	federation_signature_failures: AtomicU64,
	federation_storage_failures: AtomicU64,
	federation_rejections: AtomicU64,
	federation_forks: AtomicU64,
}

impl Metrics {
	pub fn new() -> Self {
		Self {
			started: Instant::now(),
			requests: AtomicU64::new(0),
			request_micros: AtomicU64::new(0),
			server_errors: AtomicU64::new(0),
			sessions_created: AtomicU64::new(0),
			sessions_revoked: AtomicU64::new(0),
			api_keys_created: AtomicU64::new(0),
			api_keys_revoked: AtomicU64::new(0),
			staging_collected: AtomicU64::new(0),
			blobs_collected: AtomicU64::new(0),
			blob_serve_failures: AtomicU64::new(0),
			key_changes: AtomicU64::new(0),
			signature_failures: AtomicU64::new(0),
			federation_network_failures: AtomicU64::new(0),
			federation_protocol_failures: AtomicU64::new(0),
			federation_signature_failures: AtomicU64::new(0),
			federation_storage_failures: AtomicU64::new(0),
			federation_rejections: AtomicU64::new(0),
			federation_forks: AtomicU64::new(0),
		}
	}

	pub fn record_signature_failure(&self) {
		self.signature_failures.fetch_add(1, Ordering::Relaxed);
	}

	pub fn record_request_micros(&self, micros: u64) {
		self.request_micros.fetch_add(micros, Ordering::Relaxed);
	}

	pub fn record_session_created(&self) {
		self.sessions_created.fetch_add(1, Ordering::Relaxed);
	}

	pub fn record_session_revoked(&self) {
		self.sessions_revoked.fetch_add(1, Ordering::Relaxed);
	}

	pub fn record_api_key_created(&self) {
		self.api_keys_created.fetch_add(1, Ordering::Relaxed);
	}

	pub fn record_api_key_revoked(&self) {
		self.api_keys_revoked.fetch_add(1, Ordering::Relaxed);
	}

	pub fn record_collected(&self, staging: u64, blobs: u64) {
		self.staging_collected.fetch_add(staging, Ordering::Relaxed);
		self.blobs_collected.fetch_add(blobs, Ordering::Relaxed);
	}

	pub fn record_blob_serve_failure(&self) {
		self.blob_serve_failures.fetch_add(1, Ordering::Relaxed);
	}

	pub fn record_key_change(&self) {
		self.key_changes.fetch_add(1, Ordering::Relaxed);
	}

	pub fn record_federation_failure(&self, error: &crate::federation::FederationError) {
		use crate::federation::FederationError;
		let counter = match error {
			FederationError::Http(_) => &self.federation_network_failures,
			FederationError::InvalidUrl(_) | FederationError::Decode(_) => &self.federation_protocol_failures,
			FederationError::Verify(_) => &self.federation_signature_failures,
			FederationError::Storage(_) => &self.federation_storage_failures,
			FederationError::Rejected(_) => &self.federation_rejections,
			FederationError::Fork(_) => &self.federation_forks,
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
	let started = Instant::now();
	let response = next.run(request).await;
	state.metrics.observe(response.status().as_u16());
	state.metrics.record_request_micros(started.elapsed().as_micros() as u64);
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
		(
			"moraine_federation_forks_total",
			state.metrics.federation_forks.load(Ordering::Relaxed),
		),
		(
			"moraine_sessions_created_total",
			state.metrics.sessions_created.load(Ordering::Relaxed),
		),
		(
			"moraine_sessions_revoked_total",
			state.metrics.sessions_revoked.load(Ordering::Relaxed),
		),
		(
			"moraine_api_keys_created_total",
			state.metrics.api_keys_created.load(Ordering::Relaxed),
		),
		(
			"moraine_api_keys_revoked_total",
			state.metrics.api_keys_revoked.load(Ordering::Relaxed),
		),
		(
			"moraine_staging_collected_total",
			state.metrics.staging_collected.load(Ordering::Relaxed),
		),
		(
			"moraine_blobs_collected_total",
			state.metrics.blobs_collected.load(Ordering::Relaxed),
		),
		(
			"moraine_blob_serve_failures_total",
			state.metrics.blob_serve_failures.load(Ordering::Relaxed),
		),
		("moraine_key_changes_total", state.metrics.key_changes.load(Ordering::Relaxed)),
	] {
		body.push_str("# TYPE ");
		body.push_str(name);
		body.push_str(" counter\n");
		body.push_str(name);
		body.push(' ');
		body.push_str(&value.to_string());
		body.push('\n');
	}
	let requests = state.metrics.requests.load(Ordering::Relaxed);
	let micros = state.metrics.request_micros.load(Ordering::Relaxed);
	body.push_str("# TYPE moraine_request_duration_seconds summary\n");
	body.push_str(&format!(
		"moraine_request_duration_seconds_sum {}\n",
		micros as f64 / 1_000_000.0
	));
	body.push_str(&format!("moraine_request_duration_seconds_count {requests}\n"));
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
			"moraine_federation_forks_total",
			"moraine_federation_rejections_total",
			"moraine_admission_oldest_seconds",
			"moraine_webhook_backlog_oldest_seconds",
			"moraine_subscription_lag_entries",
			"moraine_subscription_resets",
			"moraine_request_duration_seconds_sum",
			"moraine_request_duration_seconds_count",
			"moraine_sessions_created_total",
			"moraine_api_keys_revoked_total",
			"moraine_blobs_collected_total",
			"moraine_blob_serve_failures_total",
			"moraine_key_changes_total",
		] {
			assert!(text.contains(name), "missing {name}");
		}
	}
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MetricsSnapshot {
	pub projects: i64,
	pub objects: i64,
	pub submissions: i64,
	pub submissions_pending: i64,
	pub review_decisions: i64,
	pub subscriptions: i64,
	pub deliveries_pending: i64,
	pub definitions: i64,
	pub advisories: i64,
	pub mirrors: i64,
	pub artifacts: i64,
	pub subscription_lag: i64,
	pub subscription_resets: i64,
	pub oldest_pending_submission: Option<i64>,
	pub oldest_pending_delivery: Option<i64>,
}
impl MetricsSnapshot {
	pub fn lines(&self) -> Vec<(&'static str, i64)> {
		vec![
			("moraine_projects_total", self.projects),
			("moraine_objects_total", self.objects),
			("moraine_submissions_total", self.submissions),
			("moraine_submissions_pending", self.submissions_pending),
			("moraine_review_decisions_total", self.review_decisions),
			("moraine_subscriptions_total", self.subscriptions),
			("moraine_deliveries_pending", self.deliveries_pending),
			("moraine_definitions_total", self.definitions),
			("moraine_advisories_total", self.advisories),
			("moraine_mirrors_total", self.mirrors),
			("moraine_artifacts_total", self.artifacts),
			("moraine_subscription_lag_entries", self.subscription_lag),
			("moraine_subscription_resets", self.subscription_resets),
		]
	}
}

impl MetadataStore {
	pub async fn metrics_snapshot(&self) -> Result<MetricsSnapshot, sqlx::Error> {
		Ok(MetricsSnapshot {
			projects: self.table_count("projects").await?,
			objects: self.table_count("objects").await?,
			submissions: self.table_count("submissions").await?,
			submissions_pending: self
				.scalar_count("SELECT COUNT(*) FROM submissions WHERE state = 'submitted'")
				.await?,
			review_decisions: self.table_count("review_decisions").await?,
			subscriptions: self.table_count("subscriptions").await?,
			deliveries_pending: self
				.scalar_count("SELECT COUNT(*) FROM webhook_deliveries WHERE status = 'pending'")
				.await?,
			definitions: self.table_count("definitions").await?,
			advisories: self.table_count("advisories").await?,
			mirrors: self.table_count("mirrors").await?,
			artifacts: self.table_count("artifact_index").await?,
			subscription_lag: self
				.scalar_opt("SELECT MAX(remote_head_seq - cursor_seq) FROM subscriptions")
				.await?
				.unwrap_or(0),
			subscription_resets: self
				.scalar_opt("SELECT CAST(SUM(reset_count) AS BIGINT) FROM subscriptions")
				.await?
				.unwrap_or(0),
			oldest_pending_submission: self
				.scalar_opt("SELECT MIN(created_at) FROM submissions WHERE state = 'submitted'")
				.await?,
			oldest_pending_delivery: self
				.scalar_opt("SELECT MIN(next_attempt_at) FROM webhook_deliveries WHERE status = 'pending'")
				.await?,
		})
	}
}
