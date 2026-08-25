use axum::body::Body;
use axum::http::header;
use tower::ServiceExt;

use crate::test_support::{app, body_json, scope_token};

fn request(method: &str, uri: &str, token: Option<&str>, body: serde_json::Value) -> axum::http::Request<Body> {
	let mut builder = axum::http::Request::builder()
		.method(method)
		.uri(uri)
		.header(header::CONTENT_TYPE, "application/json");
	if let Some(token) = token {
		builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
	}
	builder.body(Body::from(body.to_string())).expect("request")
}

#[tokio::test]
async fn records_and_lists_a_legal_request() {
	let (application, _directory) = app().await;
	let token = scope_token(&application, "legal@example.org", "directory:manage").await;
	let project = "gd:sha256:11";

	let record = request(
		"POST",
		"/v1/legal-requests",
		Some(&token),
		serde_json::json!({
			"kind": "copyright-notice",
			"claimant_ref": "rights-holder@example.org",
			"target_kind": "project",
			"target_id": project,
			"stated_basis": "reproduces a copyrighted asset",
			"received_at": 1_760_000_000,
			"action_taken": "availability-disabled",
			"designated_agent_ref": "agent@example.org"
		}),
	);
	let response = application.clone().oneshot(record).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::CREATED);
	let recorded = body_json(response).await;
	let id = recorded["id"].as_str().expect("id").to_string();
	assert_eq!(recorded["request"]["kind"], "copyright-notice");
	assert_eq!(recorded["request"]["action_taken"], "availability-disabled");

	let counter = request(
		"POST",
		"/v1/legal-requests",
		Some(&token),
		serde_json::json!({
			"kind": "counter-notice",
			"claimant_ref": "author@example.org",
			"target_kind": "project",
			"target_id": project,
			"stated_basis": "the asset is licensed to the author",
			"received_at": 1_760_000_500,
			"responds_to": id
		}),
	);
	let response = application.clone().oneshot(counter).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::CREATED);
	assert_eq!(body_json(response).await["request"]["action_taken"], "none");

	let list = axum::http::Request::get(format!("/v1/legal-requests?target_kind=project&target_id={project}"))
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(list).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::OK);
	let records = body_json(response).await;
	assert_eq!(records.as_array().expect("records").len(), 2);

	let get = axum::http::Request::get(format!("/v1/legal-requests/{id}"))
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(get).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::OK);
	let fetched = body_json(response).await;
	assert_eq!(fetched["kind"], "copyright-notice");
	assert_eq!(fetched["stated_basis"], "reproduces a copyrighted asset");
}

#[tokio::test]
async fn refuses_a_legal_request_without_the_operator_scope() {
	let (application, _directory) = app().await;
	let anonymous = request(
		"POST",
		"/v1/legal-requests",
		None,
		serde_json::json!({
			"kind": "copyright-notice",
			"claimant_ref": "holder@example.org",
			"target_kind": "project",
			"target_id": "gd:sha256:22",
			"stated_basis": "claim",
			"received_at": 1
		}),
	);
	let response = application.clone().oneshot(anonymous).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);

	let token = scope_token(&application, "reader@example.org", "account:read").await;
	let scoped = request(
		"POST",
		"/v1/legal-requests",
		Some(&token),
		serde_json::json!({
			"kind": "copyright-notice",
			"claimant_ref": "holder@example.org",
			"target_kind": "project",
			"target_id": "gd:sha256:22",
			"stated_basis": "claim",
			"received_at": 1
		}),
	);
	let response = application.clone().oneshot(scoped).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);

	let bad = request(
		"POST",
		"/v1/legal-requests",
		Some(&scope_token(&application, "legal@example.org", "directory:manage").await),
		serde_json::json!({
			"kind": "subpoena",
			"claimant_ref": "holder@example.org",
			"target_kind": "project",
			"target_id": "gd:sha256:22",
			"stated_basis": "claim",
			"received_at": 1
		}),
	);
	let response = application.oneshot(bad).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}
