use axum::body::Body;
use axum::http::{StatusCode, header};
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
async fn records_and_lists_an_impersonation_report() {
	let (application, _directory) = app().await;
	let token = scope_token(&application, "moderator@example.org", "directory:manage").await;

	let record = request(
		"POST",
		"/v1/impersonation-reports",
		Some(&token),
		serde_json::json!({
			"claim_kind": "trademark",
			"claimant_ref": "rights-holder@example.org",
			"target_project_id": "gd:sha256:11",
			"target_handle": "official-brand",
			"evidence_ref": "https://example.org/evidence/1"
		}),
	);
	let response = application.clone().oneshot(record).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let recorded = body_json(response).await;
	let id = recorded["id"].as_str().expect("id").to_string();
	assert_eq!(recorded["status"], "open");

	let by_handle = axum::http::Request::get("/v1/impersonation-reports?handle=official-brand")
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(by_handle).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let reports = body_json(response).await;
	assert_eq!(reports.as_array().expect("reports").len(), 1);

	let by_other = axum::http::Request::get("/v1/impersonation-reports?project=gd:sha256:99")
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(by_other).await.expect("response");
	assert!(body_json(response).await.as_array().expect("reports").is_empty());

	let get = axum::http::Request::get(format!("/v1/impersonation-reports/{id}"))
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(get).await.expect("response");
	assert_eq!(body_json(response).await["claim_kind"], "trademark");

	let decided = request(
		"POST",
		"/v1/impersonation-reports",
		Some(&token),
		serde_json::json!({
			"claim_kind": "impersonation",
			"claimant_ref": "holder@example.org",
			"target_project_id": "gd:sha256:11",
			"evidence_ref": "https://example.org/evidence/2",
			"status": "upheld",
			"decided_at": 1_760_000_100
		}),
	);
	let response = application.clone().oneshot(decided).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	assert_eq!(body_json(response).await["status"], "upheld");
}

#[tokio::test]
async fn rejects_a_report_that_is_not_well_formed() {
	let (application, _directory) = app().await;
	let token = scope_token(&application, "moderator@example.org", "directory:manage").await;

	let no_target = request(
		"POST",
		"/v1/impersonation-reports",
		Some(&token),
		serde_json::json!({
			"claim_kind": "impersonation",
			"claimant_ref": "holder@example.org",
			"evidence_ref": "https://example.org/evidence"
		}),
	);
	let response = application.clone().oneshot(no_target).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);

	let undecided = request(
		"POST",
		"/v1/impersonation-reports",
		Some(&token),
		serde_json::json!({
			"claim_kind": "impersonation",
			"claimant_ref": "holder@example.org",
			"target_project_id": "gd:sha256:11",
			"evidence_ref": "https://example.org/evidence",
			"status": "rejected"
		}),
	);
	let response = application.clone().oneshot(undecided).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);

	let anonymous = request(
		"POST",
		"/v1/impersonation-reports",
		None,
		serde_json::json!({
			"claim_kind": "impersonation",
			"claimant_ref": "holder@example.org",
			"target_project_id": "gd:sha256:11",
			"evidence_ref": "https://example.org/evidence"
		}),
	);
	let response = application.oneshot(anonymous).await.expect("response");
	assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
