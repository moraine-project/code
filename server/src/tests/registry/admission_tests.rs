use axum::body::Body;
use axum::http::{StatusCode, header};
use tower::ServiceExt;

use crate::test_support::*;

#[tokio::test]
async fn review_mode_queues_then_accepts() {
	let (application, _directory) = app_review().await;
	let signer = key(1);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let (session, csrf) = login(&application, "reviewer@example.org").await;

	let feed = feed_wire(&signer, &project_id, 1, None, release_digest);
	let submit = axum::http::Request::post("/v1/submissions")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::from(feed))
		.expect("request");
	let response = application.clone().oneshot(submit).await.expect("response");
	assert_eq!(response.status(), StatusCode::ACCEPTED);
	let submission = body_json(response).await;
	let submission_id = submission["id"].as_str().expect("submission id").to_string();
	assert_eq!(submission["state"], "submitted");

	let queue = axum::http::Request::get("/v1/review-queue")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(queue).await.expect("response");
	let queued = body_json(response).await;
	assert_eq!(queued.as_array().expect("queue").len(), 1);

	let assign = axum::http::Request::post(format!("/v1/submissions/{submission_id}/assign"))
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(assign).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let queue = axum::http::Request::get("/v1/review-queue")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(queue).await.expect("response");
	let queued = body_json(response).await;
	assert_eq!(queued[0]["state"], "under_review");
	assert!(queued[0]["assigned_to"].as_str().is_some());

	let cursor = format!(
		"{}:{}",
		queued[0]["created_at"].as_i64().expect("created_at"),
		queued[0]["id"].as_str().expect("id")
	);
	let paged = axum::http::Request::get(format!("/v1/review-queue?cursor={cursor}"))
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(paged).await.expect("response");
	let queued = body_json(response).await;
	assert!(queued.as_array().expect("queue").is_empty());

	let review = axum::http::Request::post(format!("/v1/submissions/{submission_id}/review"))
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::from(serde_json::json!({ "decision": "accept" }).to_string()))
		.expect("request");
	let response = application.clone().oneshot(review).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let receipt = body_json(response).await;
	assert_eq!(receipt["state"], "accepted");

	let mine = axum::http::Request::get("/v1/submissions")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(mine).await.expect("response");
	let list = body_json(response).await;
	assert_eq!(list.as_array().expect("submissions").len(), 1);
	assert_eq!(list[0]["submission"]["state"], "accepted");
	assert_eq!(list[0]["decisions"][0]["decision"], "accept");

	let oldest = list[0]["submission"]["created_at"].as_i64().expect("created_at");
	let oldest_id = list[0]["submission"]["id"].as_str().expect("submission id");
	let older = axum::http::Request::get(format!("/v1/submissions?cursor={oldest}:{oldest_id}"))
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(older).await.expect("response");
	let page = body_json(response).await;
	assert!(page.as_array().expect("submissions").is_empty());

	let feed_request = axum::http::Request::get(format!("/v1/projects/{project_id}/feed"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(feed_request).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["head_seq"], 1);
}

#[tokio::test]
async fn open_mode_auto_accepts() {
	let (application, _directory) = app_mode(crate::config::Publishing::Open, false).await;
	let signer = key(4);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let (session, csrf) = login(&application, "author@example.org").await;

	let feed = feed_wire(&signer, &project_id, 1, None, release_digest);
	let submit = axum::http::Request::post("/v1/submissions")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(feed))
		.expect("request");
	let response = application.clone().oneshot(submit).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let receipt = body_json(response).await;
	assert_eq!(receipt["state"], "auto-accepted");

	let feed_request = axum::http::Request::get(format!("/v1/projects/{project_id}/feed"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(feed_request).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["head_seq"], 1);
}

#[tokio::test]
async fn progressive_mode_grants_after_review_and_auto_accepts_later_release() {
	let (application, _directory) = app_mode(crate::config::Publishing::Progressive, false).await;
	let signer = key(6);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let (author_session, author_csrf) = login(&application, "progressive-author@example.org").await;
	let (reviewer_session, reviewer_csrf) = login(&application, "reviewer@example.org").await;
	let cookie = format!("moraine_session={author_session}; moraine_csrf={author_csrf}");

	let first = feed_wire(&signer, &project_id, 1, None, release_digest);
	let submit = axum::http::Request::post("/v1/submissions")
		.header(header::COOKIE, &cookie)
		.header("x-csrf-token", &author_csrf)
		.body(Body::from(first))
		.expect("request");
	let response = application.clone().oneshot(submit).await.expect("response");
	assert_eq!(response.status(), StatusCode::ACCEPTED);
	let submission_id = body_json(response).await["id"].as_str().expect("submission id").to_string();

	let reviewer_cookie = format!("moraine_session={reviewer_session}; moraine_csrf={reviewer_csrf}");
	let review = axum::http::Request::post(format!("/v1/submissions/{submission_id}/review"))
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, &reviewer_cookie)
		.header("x-csrf-token", &reviewer_csrf)
		.body(Body::from(serde_json::json!({ "decision": "accept" }).to_string()))
		.expect("request");
	let response = application.clone().oneshot(review).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let receipt = body_json(response).await;
	let previous = id_bytes(receipt["entry"].as_str().expect("entry"));

	let grants = axum::http::Request::get(format!("/v1/projects/{project_id}/publication-grants"))
		.header(header::COOKIE, &reviewer_cookie)
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(grants).await.expect("response");
	let grants = body_json(response).await;
	assert_eq!(grants.as_array().expect("grants").len(), 1);
	assert!(grants[0]["suspended_at"].is_null());

	let (second, second_digest) = release_wire_variant(&signer, &project_id, 0x43, "1.0.1");
	store_release(&application, &project_id, second).await;
	let second_feed = feed_wire(&signer, &project_id, 2, Some(previous), second_digest);
	let submit = axum::http::Request::post("/v1/submissions")
		.header(header::COOKIE, &cookie)
		.header("x-csrf-token", &author_csrf)
		.body(Body::from(second_feed))
		.expect("request");
	let response = application.clone().oneshot(submit).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	assert_eq!(body_json(response).await["state"], "auto-accepted");

	let revoke = axum::http::Request::delete(format!("/v1/projects/{project_id}/publication-grants"))
		.header(header::COOKIE, &reviewer_cookie)
		.header("x-csrf-token", &reviewer_csrf)
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(revoke).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn reject_requires_a_reason_code() {
	let (application, _directory) = app_review().await;
	let signer = key(5);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let (session, csrf) = login(&application, "reviewer@example.org").await;

	let feed = feed_wire(&signer, &project_id, 1, None, release_digest);
	let submit = axum::http::Request::post("/v1/submissions")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::from(feed))
		.expect("request");
	let submission = body_json(application.clone().oneshot(submit).await.expect("response")).await;
	let submission_id = submission["id"].as_str().expect("submission id").to_string();

	let review = axum::http::Request::post(format!("/v1/submissions/{submission_id}/review"))
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(serde_json::json!({ "decision": "reject" }).to_string()))
		.expect("request");
	let response = application.oneshot(review).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
