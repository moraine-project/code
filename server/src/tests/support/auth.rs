use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::header;
use axum::response::Response;
use tower::ServiceExt;

pub(crate) async fn body_json(response: Response) -> serde_json::Value {
	let bytes = to_bytes(response.into_body(), 64 * 1024).await.expect("body");
	serde_json::from_slice(&bytes).expect("json")
}

pub(crate) fn set_cookie(response: &Response, name: &str) -> String {
	response
		.headers()
		.get_all(header::SET_COOKIE)
		.iter()
		.find_map(|value| {
			let cookie = value.to_str().ok()?;
			cookie
				.split(';')
				.next()?
				.strip_prefix(&format!("{name}="))
				.map(str::to_string)
		})
		.unwrap_or_default()
}

pub(crate) async fn login(application: &Router, email: &str) -> (String, String) {
	let credentials = serde_json::json!({ "email": email, "password": "correct horse battery" }).to_string();
	let register = axum::http::Request::post("/v1/auth/register")
		.header(header::CONTENT_TYPE, "application/json")
		.body(Body::from(credentials.clone()))
		.expect("request");
	application.clone().oneshot(register).await.expect("response");
	let login = axum::http::Request::post("/v1/auth/session")
		.header(header::CONTENT_TYPE, "application/json")
		.body(Body::from(credentials))
		.expect("request");
	let response = application.clone().oneshot(login).await.expect("response");
	let session = set_cookie(&response, "moraine_session");
	let csrf = set_cookie(&response, "moraine_csrf");
	(session, csrf)
}

pub(crate) async fn upload_token(application: &Router, email: &str) -> String {
	scope_token(application, email, "artifacts:write").await
}

pub(crate) async fn scope_token_with_user(application: &Router, email: &str, scope: &str) -> (String, String) {
	let token = scope_token(application, email, scope).await;
	let request = axum::http::Request::get("/v1/auth/me")
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let user_id = body_json(response).await["user_id"].as_str().expect("user id").to_string();
	(token, user_id)
}

pub(crate) async fn scope_token(application: &Router, email: &str, scope: &str) -> String {
	let (session, csrf) = login(application, email).await;
	let cookie = format!("moraine_session={session}; moraine_csrf={csrf}");
	let request = axum::http::Request::post("/v1/auth/keys")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, &cookie)
		.header("x-csrf-token", &csrf)
		.body(Body::from(
			serde_json::json!({ "name": "scoped", "scopes": [scope] }).to_string(),
		))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::CREATED);
	body_json(response).await["key"].as_str().expect("key").to_string()
}
