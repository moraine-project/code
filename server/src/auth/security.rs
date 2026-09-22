use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

pub(crate) fn split_scopes(scopes: &str) -> Vec<String> {
	if scopes.is_empty() {
		Vec::new()
	} else {
		scopes.split(',').map(str::to_string).collect()
	}
}

pub(crate) fn token_hash(token: &str) -> [u8; 32] {
	Sha256::digest(token.as_bytes()).into()
}

pub(crate) fn random_token() -> String {
	hex::encode(new_id_bytes())
}

pub(crate) fn new_id() -> String {
	hex::encode(new_id_bytes())
}

fn new_id_bytes() -> [u8; 16] {
	let mut bytes = [0u8; 16];
	if getrandom::fill(&mut bytes).is_err() {
		panic!("operating system randomness is unavailable");
	}
	bytes
}

pub(crate) fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

pub(crate) fn unauthorized() -> Response {
	(StatusCode::UNAUTHORIZED, "authentication required").into_response()
}

pub(crate) fn forbidden() -> Response {
	(StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response()
}

pub(crate) fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "account store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}
