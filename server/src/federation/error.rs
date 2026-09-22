use std::fmt;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug)]
pub enum FederationError {
	InvalidUrl(String),
	Http(String),
	Decode(String),
	Verify(String),
	Fork(String),
	Storage(String),
	Rejected(String),
}

impl fmt::Display for FederationError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::InvalidUrl(detail) => write!(f, "invalid home url: {detail}"),
			Self::Http(detail) => write!(f, "home request failed: {detail}"),
			Self::Decode(detail) => write!(f, "malformed home response: {detail}"),
			Self::Verify(detail) => write!(f, "home data did not verify: {detail}"),
			Self::Fork(detail) => write!(f, "the home equivocated: {detail}"),
			Self::Storage(detail) => write!(f, "local storage failed: {detail}"),
			Self::Rejected(detail) => write!(f, "home entry rejected: {detail}"),
		}
	}
}

pub(super) fn storage(error: sqlx::Error) -> FederationError {
	FederationError::Storage(error.to_string())
}

pub(super) fn rejected(error: Response) -> FederationError {
	let status = error.status();
	FederationError::Rejected(format!("local ingest returned {}", status))
}

pub(super) fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

pub(super) fn hex_of(id: &str) -> Result<String, FederationError> {
	let hex = id
		.strip_prefix("gd:sha256:")
		.ok_or_else(|| FederationError::Decode(format!("`{id}` is not an object id")))?;
	if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
		return Err(FederationError::Decode(format!("`{id}` is not an object id")));
	}
	Ok(hex.to_string())
}

pub(super) fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "subscription store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}

#[derive(Serialize)]
pub(crate) struct ResyncReport {
	pub synced: usize,
	pub failed: usize,
}
