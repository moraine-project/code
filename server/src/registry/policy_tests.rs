use axum::body::Body;
use axum::http::header;
use tower::ServiceExt;

use super::*;
use crate::test_support::*;

#[tokio::test]
async fn listing_policy_hides_a_project_from_search_and_blocks_it() {
	let (application, _directory) = app().await;
	let signer = key(26);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let profile = crate::test_support::publish_profile(&application, &signer, &project_id, "Widget").await;
	let _ = profile;
	let _ = release_digest;

	let search = |application: Router| async move {
		let request = axum::http::Request::get("/v1/search?q=widget")
			.body(Body::empty())
			.expect("request");
		let response = application.oneshot(request).await.expect("response");
		body_json(response).await
	};

	let page = search(application.clone()).await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);

	let token = crate::test_support::scope_token(&application, "moderator@example.org", "directory:manage").await;
	let unlist = axum::http::Request::builder()
		.method("PUT")
		.uri(format!("/v1/directory/policy/{project_id}"))
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::from(
			serde_json::json!({ "listing_state": "unlisted", "reason_code": "author-request" }).to_string(),
		))
		.expect("request");
	let response = application.clone().oneshot(unlist).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let page = search(application.clone()).await;
	assert!(page["results"].as_array().expect("results").is_empty());

	let get = axum::http::Request::get(format!("/v1/projects/{project_id}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(get).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["listing_state"], "unlisted");

	let quarantine = axum::http::Request::builder()
		.method("PUT")
		.uri(format!("/v1/directory/policy/{project_id}"))
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::from(
			serde_json::json!({ "listing_state": "quarantined", "reason_code": "malware-suspected" }).to_string(),
		))
		.expect("request");
	let response = application.clone().oneshot(quarantine).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let page = search(application.clone()).await;
	let results = page["results"].as_array().expect("results");
	assert_eq!(results.len(), 1);
	assert_eq!(results[0]["listing_state"], "quarantined");
	assert_eq!(results[0]["annotations"][0]["kind"], "quarantined");

	let block = axum::http::Request::builder()
		.method("PUT")
		.uri(format!("/v1/directory/policy/{project_id}"))
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::from(serde_json::json!({ "listing_state": "blocked" }).to_string()))
		.expect("request");
	let response = application.clone().oneshot(block).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let get = axum::http::Request::get(format!("/v1/projects/{project_id}"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(get).await.expect("response");
	assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
