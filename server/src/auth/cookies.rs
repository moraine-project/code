use axum::http::header;
use axum::response::Response;

use super::{ABSOLUTE_SECONDS, CSRF_COOKIE, SESSION_COOKIE};

pub(super) fn append(response: &mut Response, cookie: &str) {
	if let Ok(value) = header::HeaderValue::from_str(cookie) {
		response.headers_mut().append(header::SET_COOKIE, value);
	}
}

fn same_site(cross_origin: bool) -> &'static str {
	if cross_origin { "None" } else { "Lax" }
}

pub(super) fn session(token: &str, cross_origin: bool) -> String {
	format!(
		"{SESSION_COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite={}; Max-Age={ABSOLUTE_SECONDS}",
		same_site(cross_origin)
	)
}

pub(super) fn csrf(token: &str, cross_origin: bool) -> String {
	format!(
		"{CSRF_COOKIE}={token}; Path=/; Secure; SameSite={}; Max-Age={ABSOLUTE_SECONDS}",
		same_site(cross_origin)
	)
}

pub(super) fn clear(name: &str, http_only: bool, cross_origin: bool) -> String {
	let mut cookie = format!("{name}=; Path=/; Secure; SameSite={}; Max-Age=0", same_site(cross_origin));
	if http_only {
		cookie.push_str("; HttpOnly");
	}
	cookie
}
