use axum::body::Body;
use axum::http::{StatusCode, header};
use tower::ServiceExt;

use crate::test_support::{app, scope_token, scope_token_with_user};

fn record(token: &str, body: serde_json::Value) -> axum::http::Request<Body> {
	axum::http::Request::builder()
		.method("POST")
		.uri("/v1/sanctions")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::from(body.to_string()))
		.expect("request")
}

fn upload(token: &str) -> axum::http::Request<Body> {
	axum::http::Request::post("/v1/blobs")
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::from("artifact"))
		.expect("request")
}

#[tokio::test]
async fn a_suspension_blocks_uploads_and_a_warning_does_not() {
	let (application, _directory) = app().await;
	let moderator = scope_token(&application, "moderator@example.org", "directory:manage").await;

	let (suspended, suspended_id) = scope_token_with_user(&application, "bad@example.org", "artifacts:write").await;
	let response = application.clone().oneshot(upload(&suspended)).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let response = application
		.clone()
		.oneshot(record(
			&moderator,
			serde_json::json!({
				"subject_user_id": suspended_id,
				"kind": "suspension",
				"reason_code": "spam",
				"scope_kind": "account",
				"scope_id": suspended_id,
				"starts_at": 0
			}),
		))
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let response = application.clone().oneshot(upload(&suspended)).await.expect("response");
	assert_eq!(response.status(), StatusCode::FORBIDDEN);

	let (warned, warned_id) = scope_token_with_user(&application, "warned@example.org", "artifacts:write").await;
	let response = application
		.clone()
		.oneshot(record(
			&moderator,
			serde_json::json!({
				"subject_user_id": warned_id,
				"kind": "warning",
				"reason_code": "policy-disallowed",
				"scope_kind": "account",
				"scope_id": warned_id,
				"starts_at": 0
			}),
		))
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let response = application.clone().oneshot(upload(&warned)).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let (expired, expired_id) = scope_token_with_user(&application, "expired@example.org", "artifacts:write").await;
	let response = application
		.clone()
		.oneshot(record(
			&moderator,
			serde_json::json!({
				"subject_user_id": expired_id,
				"kind": "upload-restriction",
				"reason_code": "spam",
				"scope_kind": "account",
				"scope_id": expired_id,
				"starts_at": 0,
				"expires_at": 1
			}),
		))
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let response = application.clone().oneshot(upload(&expired)).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let list = axum::http::Request::get(format!("/v1/sanctions?user={suspended_id}"))
		.header(header::AUTHORIZATION, format!("Bearer {moderator}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(list).await.expect("response");
	let sanctions = crate::test_support::body_json(response).await;
	assert_eq!(sanctions.as_array().expect("sanctions").len(), 1);
	assert_eq!(sanctions[0]["kind"], "suspension");

	let bad = record(
		&moderator,
		serde_json::json!({
			"subject_user_id": suspended_id,
			"kind": "exile",
			"reason_code": "spam",
			"scope_kind": "account",
			"scope_id": suspended_id,
			"starts_at": 0
		}),
	);
	let response = application.oneshot(bad).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
