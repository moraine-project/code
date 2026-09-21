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
async fn records_external_catalog_data_without_making_it_native() {
	let (application, _directory) = app().await;
	let bridge = scope_token(&application, "operator@example.org", "federation:manage").await;
	let record = request(
		"PUT",
		"/v1/external-projects/modrinth/example-mod",
		Some(&bridge),
		serde_json::json!({
			"source_class": "external-catalog",
			"canonical_source_url": "https://modrinth.com/mod/example-mod",
			"observed_profile": { "name": "Example Mod", "summary": "An external record" },
			"files": [{
				"external_file_id": "file-1",
				"source_url": "https://cdn.example/mod.jar",
				"digest": "11".repeat(32),
				"size": 42,
				"metadata": { "filename": "mod.jar" },
				"observed_at": 1760000000
			}],
			"observed_at": 1760000000,
			"source_state": "active",
			"bridge_id": "test-bridge",
			"bridge_version": "1.0.0"
		}),
	);
	let response = application.clone().oneshot(record).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let public = axum::http::Request::get("/v1/external-projects/modrinth/example-mod")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(public).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["source_class"], "external-catalog");
	assert_eq!(view["files"][0]["digest"], format!("sha256:{}", "11".repeat(32)));
	assert!(view["linked_native_project_id"].is_null());
}

#[tokio::test]
async fn external_claims_are_reviewed_separately() {
	let (application, _directory) = app().await;
	let bridge = scope_token(&application, "operator@example.org", "federation:manage").await;
	let record = request(
		"PUT",
		"/v1/external-projects/modrinth/example-mod",
		Some(&bridge),
		serde_json::json!({
			"source_class": "external-catalog",
			"canonical_source_url": "https://modrinth.com/mod/example-mod",
			"observed_profile": {},
			"files": [],
			"observed_at": 1760000000,
			"source_state": "active",
			"bridge_id": "test-bridge",
			"bridge_version": "1.0.0"
		}),
	);
	assert_eq!(
		application.clone().oneshot(record).await.expect("response").status(),
		StatusCode::CREATED
	);
	let moderator = scope_token(&application, "operator@example.org", "directory:manage").await;
	let claim = request(
		"POST",
		"/v1/external-projects/modrinth/example-mod/claim",
		Some(&moderator),
		serde_json::json!({ "claimant_ref": "author@example.org", "challenge_ref": "https://modrinth.com/challenge/1" }),
	);
	let response = application.clone().oneshot(claim).await.expect("response");
	assert_eq!(response.status(), StatusCode::ACCEPTED);
	let claim = body_json(response).await;
	let id = claim["id"].as_str().expect("claim id");

	let review = request(
		"POST",
		&format!("/v1/external-project-claims/{id}/review"),
		Some(&moderator),
		serde_json::json!({ "state": "approved" }),
	);
	let response = application.clone().oneshot(review).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	assert_eq!(body_json(response).await["state"], "approved");
}

#[tokio::test]
async fn rejects_external_mirrors_without_distribution_evidence() {
	let (application, _directory) = app().await;
	let bridge = scope_token(&application, "operator@example.org", "federation:manage").await;
	let request = request(
		"PUT",
		"/v1/external-projects/provider/project",
		Some(&bridge),
		serde_json::json!({
			"source_class": "external-mirror",
			"canonical_source_url": "https://example.org/project",
			"observed_profile": {},
			"files": [],
			"observed_at": 1760000000,
			"source_state": "active",
			"bridge_id": "test-bridge",
			"bridge_version": "1.0.0"
		}),
	);
	assert_eq!(
		application.oneshot(request).await.expect("response").status(),
		StatusCode::BAD_REQUEST
	);
}
