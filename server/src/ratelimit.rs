use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

use crate::routes::AppState;

const WINDOW_SECONDS: i64 = 60;
const MAX_TRACKED_KEYS: usize = 10_000;

pub struct RateLimiter {
	windows: Mutex<HashMap<String, (u32, i64)>>,
}

impl RateLimiter {
	pub fn new() -> Self {
		Self {
			windows: Mutex::new(HashMap::new()),
		}
	}

	fn check(&self, key: &str, limit: u32, now: i64) -> Result<(), i64> {
		if limit == 0 {
			return Ok(());
		}
		let mut windows = self.windows.lock().expect("rate limiter");
		if windows.len() > MAX_TRACKED_KEYS {
			windows.retain(|_, (_, start)| now - *start < WINDOW_SECONDS);
		}
		let entry = windows.entry(key.to_string()).or_insert((0, now));
		if now - entry.1 >= WINDOW_SECONDS {
			*entry = (0, now);
		}
		if entry.0 >= limit {
			return Err((entry.1 + WINDOW_SECONDS - now).max(1));
		}
		entry.0 += 1;
		Ok(())
	}
}

impl Default for RateLimiter {
	fn default() -> Self {
		Self::new()
	}
}

pub async fn limit(State(state): State<AppState>, request: Request, next: Next) -> Response {
	let path = request.uri().path();
	if matches!(path, "/healthz" | "/readyz" | "/metrics") {
		return next.run(request).await;
	}
	let limit = state.capability.requests_per_minute;
	let key = client_key(&request);
	if let Err(retry_after) = state.rate_limiter.check(&key, limit, unix_now()) {
		let mut response = (StatusCode::TOO_MANY_REQUESTS, "too many requests").into_response();
		response
			.headers_mut()
			.insert(header::RETRY_AFTER, retry_after.to_string().parse().expect("valid header"));
		return response;
	}
	next.run(request).await
}

fn client_key(request: &Request) -> String {
	if let Some(value) = request
		.headers()
		.get(header::AUTHORIZATION)
		.and_then(|value| value.to_str().ok())
	{
		return format!("token:{}", digest(value.as_bytes()));
	}
	if let Some(cookie) = request.headers().get(header::COOKIE).and_then(|value| value.to_str().ok())
		&& let Some(token) = cookie
			.split(';')
			.find_map(|part| part.trim().strip_prefix("moraine_session="))
	{
		return format!("session:{}", digest(token.as_bytes()));
	}
	let Some(ConnectInfo(address)) = request.extensions().get::<ConnectInfo<SocketAddr>>() else {
		return "anonymous".to_string();
	};
	let address = if address.ip().is_loopback() {
		forwarded_ip(request).unwrap_or(address.ip())
	} else {
		address.ip()
	};
	format!("ip:{address}")
}

fn forwarded_ip(request: &Request) -> Option<IpAddr> {
	request
		.headers()
		.get("x-forwarded-for")?
		.to_str()
		.ok()?
		.split(',')
		.next()?
		.trim()
		.parse()
		.ok()
}

fn digest(value: &[u8]) -> String {
	hex::encode(Sha256::digest(value))
}

fn unix_now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|elapsed| elapsed.as_secs() as i64)
		.unwrap_or(0)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn refuses_past_the_budget_and_recovers_with_the_window() {
		let limiter = RateLimiter::new();
		assert!(limiter.check("key", 2, 100).is_ok());
		assert!(limiter.check("key", 2, 100).is_ok());
		let retry = limiter.check("key", 2, 100).expect_err("over budget");
		assert!(retry > 0);
		assert!(limiter.check("other", 2, 100).is_ok());
		assert!(limiter.check("key", 2, 100 + WINDOW_SECONDS).is_ok());
	}

	#[test]
	fn keys_on_the_forwarded_address() {
		let request = axum::http::Request::get("/")
			.header("x-forwarded-for", "203.0.113.9, 10.0.0.1")
			.body(axum::body::Body::empty())
			.expect("request");
		assert_eq!(forwarded_ip(&request), Some("203.0.113.9".parse().expect("address")));
	}

	#[test]
	fn keys_on_a_bearer_token_before_the_address() {
		let request = axum::http::Request::get("/")
			.header(header::AUTHORIZATION, "Bearer secret")
			.body(axum::body::Body::empty())
			.expect("request");
		assert_eq!(client_key(&request), format!("token:{}", digest(b"Bearer secret")));
	}

	#[test]
	fn a_zero_budget_disables_limiting() {
		let limiter = RateLimiter::new();
		for _ in 0..100 {
			assert!(limiter.check("key", 0, 100).is_ok());
		}
	}
}

#[cfg(test)]
mod integration {
	use tower::ServiceExt;

	use crate::test_support::app_with_rate_limit;

	#[tokio::test]
	async fn refuses_a_burst_from_one_client() {
		let (application, _directory) = app_with_rate_limit(2).await;
		for _ in 0..2 {
			let request = axum::http::Request::get("/v1/search")
				.body(axum::body::Body::empty())
				.expect("request");
			let response = application.clone().oneshot(request).await.expect("response");
			assert_eq!(response.status(), axum::http::StatusCode::OK);
		}
		let request = axum::http::Request::get("/v1/search")
			.body(axum::body::Body::empty())
			.expect("request");
		let response = application.clone().oneshot(request).await.expect("response");
		assert_eq!(response.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
		assert!(response.headers().get(axum::http::header::RETRY_AFTER).is_some());

		let request = axum::http::Request::get("/healthz")
			.body(axum::body::Body::empty())
			.expect("request");
		let response = application.oneshot(request).await.expect("response");
		assert_eq!(response.status(), axum::http::StatusCode::OK);
	}
}
