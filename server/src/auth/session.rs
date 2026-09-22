use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{Method, StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::security::{now, split_scopes, storage_error, token_hash, unauthorized};
use super::{CSRF_COOKIE, IDLE_SECONDS, KNOWN_SCOPES, OPERATOR_ROLE, SESSION_COOKIE, SESSION_SCOPES};
use crate::routes::AppState;

pub struct AuthenticatedUser {
	pub user_id: String,
	pub scopes: Vec<String>,
	pub session_id: Option<String>,
	pub api_key_id: Option<String>,
	pub is_operator: bool,
}

impl AuthenticatedUser {
	pub fn allows(&self, scope: &str) -> bool {
		if self.session_id.is_some() {
			return self.is_operator || SESSION_SCOPES.contains(&scope);
		}
		self.scopes.iter().any(|granted| granted == scope)
	}

	pub fn mintable_scopes(&self) -> Vec<&str> {
		if self.session_id.is_some() && self.is_operator {
			return KNOWN_SCOPES.to_vec();
		}
		if self.session_id.is_some() {
			return SESSION_SCOPES.to_vec();
		}
		self.scopes.iter().map(String::as_str).collect()
	}

	pub fn via(&self) -> &'static str {
		if self.api_key_id.is_some() { "api-key" } else { "session" }
	}
}

impl FromRequestParts<AppState> for AuthenticatedUser {
	type Rejection = Response;

	async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
		let now = now();
		if let Some(token) = bearer_token(parts) {
			let key = state
				.metadata
				.api_key_by_token(&token_hash(&token), now)
				.await
				.map_err(storage_error)?;
			let Some(key) = key else {
				return Err(unauthorized());
			};
			let _ = state.metadata.touch_api_key(&key.id, now).await;
			return Ok(Self {
				user_id: key.user_id,
				scopes: split_scopes(&key.scopes),
				session_id: None,
				api_key_id: Some(key.id),
				is_operator: false,
			});
		}
		if let Some(token) = cookie(parts.headers.get(header::COOKIE), SESSION_COOKIE) {
			let session = state
				.metadata
				.session_by_token(&token_hash(&token), now)
				.await
				.map_err(storage_error)?;
			let Some(session) = session else {
				return Err(unauthorized());
			};
			if is_mutating(&parts.method) {
				let header_token = parts.headers.get("x-csrf-token").and_then(|value| value.to_str().ok());
				let cookie_token = cookie(parts.headers.get(header::COOKIE), CSRF_COOKIE);
				if header_token.is_none() || header_token != cookie_token.as_deref() {
					return Err((StatusCode::FORBIDDEN, "missing or mismatched CSRF token").into_response());
				}
				if !origin_trusted(parts, state) {
					return Err((StatusCode::FORBIDDEN, "request origin is not allowed").into_response());
				}
			}
			let _ = state.metadata.touch_session(&session.id, now, now + IDLE_SECONDS).await;
			let role = state.metadata.user_role(&session.user_id).await.map_err(storage_error)?;
			return Ok(Self {
				user_id: session.user_id,
				scopes: Vec::new(),
				session_id: Some(session.id),
				api_key_id: None,
				is_operator: role.as_deref() == Some(OPERATOR_ROLE),
			});
		}
		Err(unauthorized())
	}
}

pub struct ClientIp(pub Option<std::net::IpAddr>);

impl FromRequestParts<AppState> for ClientIp {
	type Rejection = std::convert::Infallible;

	async fn from_request_parts(parts: &mut Parts, _state: &AppState) -> Result<Self, Self::Rejection> {
		let ip = parts
			.extensions
			.get::<ConnectInfo<std::net::SocketAddr>>()
			.map(|ConnectInfo(address)| address.ip());
		Ok(Self(ip))
	}
}

fn bearer_token(parts: &Parts) -> Option<String> {
	let value = parts.headers.get(header::AUTHORIZATION)?.to_str().ok()?;
	value.strip_prefix("Bearer ").map(str::to_string)
}

fn cookie(header: Option<&axum::http::HeaderValue>, name: &str) -> Option<String> {
	let header = header?.to_str().ok()?;
	header
		.split(';')
		.filter_map(|pair| pair.trim().split_once('='))
		.find(|(key, _)| *key == name)
		.map(|(_, value)| value.to_string())
}

fn is_mutating(method: &Method) -> bool {
	matches!(*method, Method::POST | Method::PUT | Method::PATCH | Method::DELETE)
}

fn origin_trusted(parts: &Parts, state: &AppState) -> bool {
	let origin = parts
		.headers
		.get(header::ORIGIN)
		.and_then(|value| value.to_str().ok())
		.or_else(|| parts.headers.get(header::REFERER).and_then(|value| value.to_str().ok()));
	let Some(origin) = origin else {
		return true;
	};
	if state.capability.origin_allowed(origin) {
		return true;
	}
	let Some(host) = parts.headers.get(header::HOST).and_then(|value| value.to_str().ok()) else {
		return true;
	};
	origin_host(origin) == Some(host)
}

fn origin_host(origin: &str) -> Option<&str> {
	let rest = origin.split_once("://").map_or(origin, |(_, rest)| rest);
	rest.split('/').next()
}
