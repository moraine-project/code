use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method, StatusCode, header};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;

use crate::routes::AppState;

const MAX_REQUEST_BODY_BYTES: usize = 256 * 1024;
const REQUEST_TIMEOUT_SECONDS: u64 = 30;

pub(super) fn apply(app: Router<AppState>, state: AppState, web_origins: &[String]) -> Router {
	app.layer(axum::middleware::from_fn_with_state(
		state.clone(),
		crate::ops::metrics::track,
	))
	.layer(axum::middleware::from_fn_with_state(
		state.clone(),
		crate::auth::ratelimit::limit,
	))
	.with_state(state)
	.layer(cors(web_origins))
	.layer(DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
	.layer(TimeoutLayer::with_status_code(
		StatusCode::REQUEST_TIMEOUT,
		Duration::from_secs(REQUEST_TIMEOUT_SECONDS),
	))
	.layer(SetResponseHeaderLayer::if_not_present(
		header::CONTENT_SECURITY_POLICY,
		HeaderValue::from_static("frame-ancestors 'none'; base-uri 'self'; object-src 'none'"),
	))
	.layer(SetResponseHeaderLayer::if_not_present(
		header::X_CONTENT_TYPE_OPTIONS,
		HeaderValue::from_static("nosniff"),
	))
	.layer(SetResponseHeaderLayer::if_not_present(
		header::REFERRER_POLICY,
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
