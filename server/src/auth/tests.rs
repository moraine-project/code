use std::sync::Arc;

use axum::body::{Body, to_bytes};
use tower::ServiceExt;

use super::*;
use crate::blob::BlobStore;
use crate::capability::Capability;
use crate::db::MetadataStore;

async fn app() -> (Router, tempfile::TempDir) {
	let directory = tempfile::tempdir().expect("tempdir");
	let store = Arc::new(BlobStore::new(directory.path()).await.expect("blob store"));
	let metadata = Arc::new(
		MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("metadata"),
	);
	crate::test_support::seed_operators(&metadata).await;
	let config = crate::config::Config {
		bind: "127.0.0.1:0".parse().expect("addr"),
		data_dir: directory.path().to_path_buf(),
		max_artifact_bytes: 1024,
		max_upload_bytes_per_account: 5_368_709_120,
		max_projects: 10_000,
		tls_terminated: false,
		allow_insecure_http: false,
		web_origins: Vec::new(),
		registration: crate::config::Registration::Open,
		max_definitions: 1_000,
		max_sync_entries: 10_000,
		metrics_token: None,
		smtp_url: None,
		mail_from: None,
		public_url: None,
		require_verified_email: false,
		max_mirror_probes_per_cycle: 20,
		max_mirror_probe_bytes: 268_435_456,
		max_feed_page_entries: 100,
		max_response_bytes: 16_777_216,
		staging_retention_seconds: 3_600,
		blob_retention_seconds: 604_800,
		max_sync_pages: 200,
		requests_per_minute: 600,
		max_concurrent_syncs: 4,
		maintenance_interval_seconds: 3_600,
		tls_extra_roots: None,
		max_feed_scan_pages: 50,
		skip_migrate_on_start: false,
		database_url: None,
		allow_insecure_federation_local: false,
		publishing: crate::config::Publishing::Review,
		web_dir: None,
		s3: Default::default(),
	};
	let state = AppState {
		store,
		metadata,
		capability: Arc::new(Capability::discover(&config)),
		login_limiter: Arc::new(crate::auth::LoginLimiter::new()),
		metrics: Arc::new(crate::ops::metrics::Metrics::new()),
		rate_limiter: Arc::new(crate::auth::ratelimit::RateLimiter::new()),
		web_dir: None,
	};
	(crate::routes::router(state), directory)
}

fn json_request(method: &str, path: &str, body: serde_json::Value) -> axum::http::Request<Body> {
	axum::http::Request::builder()
		.method(method)
		.uri(path)
		.header(header::CONTENT_TYPE, "application/json")
		.body(Body::from(body.to_string()))
		.expect("request")
}

fn cookie_token(response: &Response, name: &str) -> Option<String> {
	response.headers().get_all(header::SET_COOKIE).iter().find_map(|value| {
		let cookie = value.to_str().ok()?;
		let first = cookie.split(';').next()?;
		first.strip_prefix(&format!("{name}=")).map(str::to_string)
	})
}

async fn json_body(response: Response) -> serde_json::Value {
	let bytes = to_bytes(response.into_body(), 64 * 1024).await.expect("body");
	serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

#[tokio::test]
async fn registers_logs_in_and_lists_a_key() {
	let (application, _directory) = app().await;
	let register = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "author@example.org", "password": "correct horse battery" }),
	);
	let response = application.clone().oneshot(register).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let body = json_body(response).await;
	assert!(body["user_id"].is_string());

	let duplicate = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "author@example.org", "password": "correct horse battery" }),
	);
	let response = application.clone().oneshot(duplicate).await.expect("response");
	assert_eq!(response.status(), StatusCode::CONFLICT);

	let login = json_request(
		"POST",
		"/v1/auth/session",
		serde_json::json!({ "email": "author@example.org", "password": "correct horse battery" }),
	);
	let response = application.clone().oneshot(login).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let session = cookie_token(&response, SESSION_COOKIE).expect("session cookie");
	let csrf = cookie_token(&response, CSRF_COOKIE).expect("csrf cookie");

	let me = axum::http::Request::builder()
		.method("GET")
		.uri("/v1/auth/me")
		.header(header::COOKIE, format!("{SESSION_COOKIE}={session}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(me).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let body = json_body(response).await;
	assert_eq!(body["via"], "session");

	let create = axum::http::Request::builder()
		.method("POST")
		.uri("/v1/auth/keys")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("{SESSION_COOKIE}={session}; {CSRF_COOKIE}={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::from(
			serde_json::json!({ "name": "ci", "scopes": ["keys:manage"] }).to_string(),
		))
		.expect("request");
	let response = application.clone().oneshot(create).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let body = json_body(response).await;
	let secret = body["key"].as_str().expect("key").to_string();

	let me_via_key = axum::http::Request::builder()
		.method("GET")
		.uri("/v1/auth/me")
		.header(header::AUTHORIZATION, format!("Bearer {secret}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(me_via_key).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let body = json_body(response).await;
	assert_eq!(body["via"], "api-key");
}

#[tokio::test]
async fn rejects_cookie_write_without_csrf_and_requires_authentication() {
	let (application, _directory) = app().await;
	let me = axum::http::Request::get("/v1/auth/me").body(Body::empty()).expect("request");
	let response = application.clone().oneshot(me).await.expect("response");
	assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

	let register = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "a@b.org", "password": "correct horse battery" }),
	);
	application.clone().oneshot(register).await.expect("response");
	let login = json_request(
		"POST",
		"/v1/auth/session",
		serde_json::json!({ "email": "a@b.org", "password": "correct horse battery" }),
	);
	let response = application.clone().oneshot(login).await.expect("response");
	let session = cookie_token(&response, SESSION_COOKIE).expect("session cookie");

	let create = axum::http::Request::builder()
		.method("POST")
		.uri("/v1/auth/keys")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("{SESSION_COOKIE}={session}"))
		.body(Body::from(serde_json::json!({ "name": "ci" }).to_string()))
		.expect("request");
	let response = application.oneshot(create).await.expect("response");
	assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn login_returns_the_csrf_token_in_the_body_for_cross_origin_clients() {
	let (application, _directory) = app().await;
	let register = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "cross@example.org", "password": "correct horse battery" }),
	);
	application.clone().oneshot(register).await.expect("response");
	let login = json_request(
		"POST",
		"/v1/auth/session",
		serde_json::json!({ "email": "cross@example.org", "password": "correct horse battery" }),
	);
	let response = application.clone().oneshot(login).await.expect("response");
	let csrf_cookie = cookie_token(&response, CSRF_COOKIE).expect("csrf cookie");
	let body = json_body(response).await;
	assert_eq!(body["csrf"].as_str(), Some(csrf_cookie.as_str()));
}

#[tokio::test]
async fn a_foreign_origin_cannot_drive_a_session_write() {
	let (application, _directory) = app().await;
	let register = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "origin@example.org", "password": "correct horse battery" }),
	);
	application.clone().oneshot(register).await.expect("response");
	let login = json_request(
		"POST",
		"/v1/auth/session",
		serde_json::json!({ "email": "origin@example.org", "password": "correct horse battery" }),
	);
	let response = application.clone().oneshot(login).await.expect("response");
	let session = cookie_token(&response, SESSION_COOKIE).expect("session cookie");
	let csrf = cookie_token(&response, CSRF_COOKIE).expect("csrf cookie");

	let create = |origin: &str| {
		axum::http::Request::builder()
			.method("POST")
			.uri("/v1/auth/keys")
			.header(header::CONTENT_TYPE, "application/json")
			.header(header::HOST, "api.example")
			.header(header::ORIGIN, origin)
			.header(header::COOKIE, format!("{SESSION_COOKIE}={session}; {CSRF_COOKIE}={csrf}"))
			.header("x-csrf-token", csrf.clone())
			.body(Body::from(serde_json::json!({ "name": "ci" }).to_string()))
			.expect("request")
	};

	let response = application
		.clone()
		.oneshot(create("https://evil.example"))
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::FORBIDDEN);

	let response = application
		.clone()
		.oneshot(create("https://api.example"))
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn a_member_session_is_not_an_operator() {
	let (application, _directory) = app().await;
	let register = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "member@example.org", "password": "correct horse battery" }),
	);
	application.clone().oneshot(register).await.expect("response");
	let login = json_request(
		"POST",
		"/v1/auth/session",
		serde_json::json!({ "email": "member@example.org", "password": "correct horse battery" }),
	);
	let response = application.clone().oneshot(login).await.expect("response");
	let session = cookie_token(&response, SESSION_COOKIE).expect("session cookie");
	let csrf = cookie_token(&response, CSRF_COOKIE).expect("csrf cookie");

	let queue = axum::http::Request::get("/v1/review-queue")
		.header(header::COOKIE, format!("{SESSION_COOKIE}={session}; {CSRF_COOKIE}={csrf}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(queue).await.expect("response");
	assert_eq!(response.status(), StatusCode::FORBIDDEN, "a member is not a reviewer");

	let mint = axum::http::Request::builder()
		.method("POST")
		.uri("/v1/auth/keys")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("{SESSION_COOKIE}={session}; {CSRF_COOKIE}={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(
			serde_json::json!({ "name": "grab", "scopes": ["directory:manage"] }).to_string(),
		))
		.expect("request");
	let response = application.oneshot(mint).await.expect("response");
	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"a member cannot mint an operator scope"
	);
}

#[tokio::test]
async fn an_operator_session_reaches_an_operator_route() {
	let (application, _directory) = app().await;
	let login = json_request(
		"POST",
		"/v1/auth/session",
		serde_json::json!({ "email": "ops@example.org", "password": "correct horse battery" }),
	);
	let response = application.clone().oneshot(login).await.expect("response");
	let session = cookie_token(&response, SESSION_COOKIE).expect("session cookie");
	let csrf = cookie_token(&response, CSRF_COOKIE).expect("csrf cookie");

	let queue = axum::http::Request::get("/v1/review-queue")
		.header(header::COOKIE, format!("{SESSION_COOKIE}={session}; {CSRF_COOKIE}={csrf}"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(queue).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
}

async fn sign_in(application: &Router, email: &str, password: &str) -> (String, String) {
	let login = json_request(
		"POST",
		"/v1/auth/session",
		serde_json::json!({ "email": email, "password": password }),
	);
	let response = application.clone().oneshot(login).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK, "sign in as {email}");
	let session = cookie_token(&response, SESSION_COOKIE).expect("session cookie");
	let csrf = cookie_token(&response, CSRF_COOKIE).expect("csrf cookie");
	(session, csrf)
}

fn session_request(
	method: &str,
	path: &str,
	session: &str,
	csrf: &str,
	body: serde_json::Value,
) -> axum::http::Request<Body> {
	axum::http::Request::builder()
		.method(method)
		.uri(path)
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("{SESSION_COOKIE}={session}; {CSRF_COOKIE}={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(body.to_string()))
		.expect("request")
}

#[tokio::test]
async fn changes_a_password_and_revokes_other_sessions() {
	let (application, _directory) = app().await;
	let register = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "rotate@example.org", "password": "correct horse battery" }),
	);
	application.clone().oneshot(register).await.expect("register");
	let (first, _) = sign_in(&application, "rotate@example.org", "correct horse battery").await;
	let (second, second_csrf) = sign_in(&application, "rotate@example.org", "correct horse battery").await;

	let wrong = session_request(
		"POST",
		"/v1/auth/password",
		&second,
		&second_csrf,
		serde_json::json!({ "current": "not the password", "new": "a much longer password" }),
	);
	let response = application.clone().oneshot(wrong).await.expect("response");
	assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

	let change = session_request(
		"POST",
		"/v1/auth/password",
		&second,
		&second_csrf,
		serde_json::json!({ "current": "correct horse battery", "new": "a much longer password" }),
	);
	let response = application.clone().oneshot(change).await.expect("response");
	assert_eq!(response.status(), StatusCode::NO_CONTENT);

	let stale = axum::http::Request::get("/v1/auth/me")
		.header(header::COOKIE, format!("{SESSION_COOKIE}={first}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(stale).await.expect("response");
	assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "the other session was revoked");

	let old = json_request(
		"POST",
		"/v1/auth/session",
		serde_json::json!({ "email": "rotate@example.org", "password": "correct horse battery" }),
	);
	let response = application.clone().oneshot(old).await.expect("response");
	assert_eq!(
		response.status(),
		StatusCode::UNAUTHORIZED,
		"the old password no longer works"
	);

	let _ = sign_in(&application, "rotate@example.org", "a much longer password").await;
}

#[tokio::test]
async fn recovers_an_account_with_a_recovery_code() {
	let (application, _directory) = app().await;
	let register = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "lost@example.org", "password": "correct horse battery" }),
	);
	application.clone().oneshot(register).await.expect("register");
	let (session, csrf) = sign_in(&application, "lost@example.org", "correct horse battery").await;

	let issue = session_request("POST", "/v1/auth/recovery-codes", &session, &csrf, serde_json::json!({}));
	let response = application.clone().oneshot(issue).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let body = json_body(response).await;
	assert_eq!(body["codes"].as_array().expect("codes").len(), 8);
	let code = body["codes"][0].as_str().expect("code").to_string();

	let recover = json_request(
		"POST",
		"/v1/auth/recover",
		serde_json::json!({ "email": "lost@example.org", "code": code, "new": "a brand new long password" }),
	);
	let response = application.clone().oneshot(recover).await.expect("response");
	assert_eq!(response.status(), StatusCode::NO_CONTENT);

	let _ = sign_in(&application, "lost@example.org", "a brand new long password").await;

	let reuse = json_request(
		"POST",
		"/v1/auth/recover",
		serde_json::json!({ "email": "lost@example.org", "code": code, "new": "yet another long password" }),
	);
	let response = application.oneshot(reuse).await.expect("response");
	assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "a code is single use");
}

#[tokio::test]
async fn an_operator_resets_a_members_password() {
	let (application, _directory) = app().await;
	let register = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "forgetful@example.org", "password": "correct horse battery" }),
	);
	application.clone().oneshot(register).await.expect("register");

	let member = json_request(
		"POST",
		"/v1/auth/session",
		serde_json::json!({ "email": "member@example.org", "password": "correct horse battery" }),
	);
	application.clone().oneshot(member).await.expect("response");
	let member_register = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "member@example.org", "password": "correct horse battery" }),
	);
	application.clone().oneshot(member_register).await.expect("register");
	let (member_session, member_csrf) = sign_in(&application, "member@example.org", "correct horse battery").await;
	let denied = session_request(
		"POST",
		"/v1/auth/users/reset-password",
		&member_session,
		&member_csrf,
		serde_json::json!({ "email": "forgetful@example.org" }),
	);
	let response = application.clone().oneshot(denied).await.expect("response");
	assert_eq!(response.status(), StatusCode::FORBIDDEN);

	let (operator_session, operator_csrf) = sign_in(&application, "ops@example.org", "correct horse battery").await;
	let reset = session_request(
		"POST",
		"/v1/auth/users/reset-password",
		&operator_session,
		&operator_csrf,
		serde_json::json!({ "email": "forgetful@example.org" }),
	);
	let response = application.clone().oneshot(reset).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let password = json_body(response).await["password"].as_str().expect("password").to_string();

	let _ = sign_in(&application, "forgetful@example.org", &password).await;
}

#[tokio::test]
async fn an_operator_creates_and_deletes_an_account() {
	let (application, _directory) = app().await;
	let (session, csrf) = sign_in(&application, "ops@example.org", "correct horse battery").await;

	let create = session_request(
		"POST",
		"/v1/auth/users",
		&session,
		&csrf,
		serde_json::json!({ "email": "invited@example.org", "role": "member" }),
	);
	let response = application.clone().oneshot(create).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let body = json_body(response).await;
	let user_id = body["user_id"].as_str().expect("user id").to_string();
	let password = body["password"].as_str().expect("password").to_string();
	let _ = sign_in(&application, "invited@example.org", &password).await;

	let list = axum::http::Request::get("/v1/auth/users")
		.header(header::COOKIE, format!("{SESSION_COOKIE}={session}; {CSRF_COOKIE}={csrf}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(list).await.expect("response");
	let accounts = json_body(response).await;
	assert!(
		accounts
			.as_array()
			.expect("accounts")
			.iter()
			.any(|account| account["email"] == "invited@example.org")
	);

	let delete = axum::http::Request::builder()
		.method("DELETE")
		.uri(format!("/v1/auth/users/{user_id}"))
		.header(header::COOKIE, format!("{SESSION_COOKIE}={session}; {CSRF_COOKIE}={csrf}"))
		.header("x-csrf-token", &csrf)
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(delete).await.expect("response");
	assert_eq!(response.status(), StatusCode::NO_CONTENT);

	let gone = json_request(
		"POST",
		"/v1/auth/session",
		serde_json::json!({ "email": "invited@example.org", "password": password }),
	);
	let response = application.oneshot(gone).await.expect("response");
	assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "the account is gone");
}

#[tokio::test]
async fn a_member_cannot_manage_accounts() {
	let (application, _directory) = app().await;
	let register = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "plain@example.org", "password": "correct horse battery" }),
	);
	application.clone().oneshot(register).await.expect("register");
	let (session, csrf) = sign_in(&application, "plain@example.org", "correct horse battery").await;
	let create = session_request(
		"POST",
		"/v1/auth/users",
		&session,
		&csrf,
		serde_json::json!({ "email": "sneaky@example.org" }),
	);
	let response = application.oneshot(create).await.expect("response");
	assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn exports_and_deletes_your_own_account() {
	let (application, _directory) = app().await;
	let register = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "leaving@example.org", "password": "correct horse battery" }),
	);
	application.clone().oneshot(register).await.expect("register");
	let (session, csrf) = sign_in(&application, "leaving@example.org", "correct horse battery").await;

	let export = axum::http::Request::get("/v1/auth/export")
		.header(header::COOKIE, format!("{SESSION_COOKIE}={session}; {CSRF_COOKIE}={csrf}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(export).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let body = json_body(response).await;
	assert_eq!(body["account"]["email"], "leaving@example.org");
	assert!(body["organizations"].is_array());

	let delete = axum::http::Request::builder()
		.method("DELETE")
		.uri("/v1/auth/me")
		.header(header::COOKIE, format!("{SESSION_COOKIE}={session}; {CSRF_COOKIE}={csrf}"))
		.header("x-csrf-token", &csrf)
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(delete).await.expect("response");
	assert_eq!(response.status(), StatusCode::NO_CONTENT);

	let gone = json_request(
		"POST",
		"/v1/auth/session",
		serde_json::json!({ "email": "leaving@example.org", "password": "correct horse battery" }),
	);
	let response = application.oneshot(gone).await.expect("response");
	assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_is_rate_limited_per_account() {
	let (application, _directory) = app().await;
	let register = json_request(
		"POST",
		"/v1/auth/register",
		serde_json::json!({ "email": "limit@example.org", "password": "correct horse battery" }),
	);
	application.clone().oneshot(register).await.expect("response");

	for _ in 0..10 {
		let wrong = json_request(
			"POST",
			"/v1/auth/session",
			serde_json::json!({ "email": "limit@example.org", "password": "wrong password here" }),
		);
		let response = application.clone().oneshot(wrong).await.expect("response");
		assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
	}

	let correct = json_request(
		"POST",
		"/v1/auth/session",
		serde_json::json!({ "email": "limit@example.org", "password": "correct horse battery" }),
	);
	let response = application.oneshot(correct).await.expect("response");
	assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
	assert!(response.headers().get(header::RETRY_AFTER).is_some());
}
