use axum::body::Body;
use moraine_crypto::{ObjectKind as Kind, object_id};
use moraine_model::release::Withdrawal;
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use super::*;
use crate::test_support::*;

#[tokio::test]
async fn publishes_project_object_and_feed_end_to_end() {
	let (application, _directory) = app().await;
	let signer = key(1);

	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire(&signer, PROJECT_KINDS)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let receipt = body_json(response).await;
	let project_id = receipt["project_id"].as_str().expect("project id").to_string();

	let (release, release_digest) = release_wire(&signer, &project_id);
	let path = format!("/v1/projects/{project_id}/objects/release");
	let request = axum::http::Request::post(&path).body(Body::from(release)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let feed = feed_wire(&signer, &project_id, 1, None, release_digest);
	let path = format!("/v1/projects/{project_id}/feed");
	let request = axum::http::Request::post(&path).body(Body::from(feed)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let receipt = body_json(response).await;
	assert_eq!(receipt["seq"], 1);

	let request = axum::http::Request::get(&path).body(Body::empty()).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["entries"].as_array().expect("entries").len(), 1);
	assert_eq!(page["head_seq"], 1);
	assert_eq!(page["entries"][0]["title"], "1.0.0 (release)");
	assert_eq!(page["entries"][0]["release"]["channel"], "release");
	assert_eq!(page["entries"][0]["release"]["game_id"], sample_id("minecraft"));
	assert_eq!(page["entries"][0]["release"]["loaders"].as_array().expect("loaders").len(), 1);

	let matching = axum::http::Request::get(format!("/v1/projects/{project_id}/feed?game_version=1.20.1"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(matching).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["entries"].as_array().expect("entries").len(), 1);

	let other = axum::http::Request::get(format!("/v1/projects/{project_id}/feed?game_version=1.19.0"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(other).await.expect("response");
	let page = body_json(response).await;
	assert!(page["entries"].as_array().expect("entries").is_empty());

	let object_path = format!("/v1/objects/{}", hex::encode(release_digest));
	let request = axum::http::Request::get(&object_path).body(Body::empty()).expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn rejects_a_feed_gap() {
	let (application, _directory) = app().await;
	let signer = key(2);
	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire(&signer, PROJECT_KINDS)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let receipt = body_json(response).await;
	let project_id = receipt["project_id"].as_str().expect("project id").to_string();

	let (release, release_digest) = release_wire(&signer, &project_id);
	let path = format!("/v1/projects/{project_id}/objects/release");
	let request = axum::http::Request::post(&path).body(Body::from(release)).expect("request");
	application.clone().oneshot(request).await.expect("response");

	let feed = feed_wire(&signer, &project_id, 2, Some([0u8; 32]), release_digest);
	let path = format!("/v1/projects/{project_id}/feed");
	let request = axum::http::Request::post(&path).body(Body::from(feed)).expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn review_mode_refuses_direct_feed_append() {
	let (application, _directory) = app_review().await;
	let signer = key(9);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let feed = feed_wire(&signer, &project_id, 1, None, release_digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(feed))
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_filtered_page_reads_past_a_full_page_of_misses() {
	let (application, _directory) = app().await;
	let signer = key(4);
	let (project_id, first_release) = publish_project(&application, &signer).await;
	let first = feed_wire(&signer, &project_id, 1, None, first_release);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(first))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let previous = id_bytes(body_json(response).await["entry"].as_str().expect("entry id"));

	let (release, digest) = release_wire_for_game(&signer, &project_id, 0x71, "2.0.0", "1.19.0");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(release))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);
	let second = feed_wire(&signer, &project_id, 2, Some(previous), digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(second))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);

	let request = axum::http::Request::get(format!("/v1/projects/{project_id}/feed?limit=1&game_version=1.19.0"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	let page = body_json(response).await;
	let entries = page["entries"].as_array().expect("entries");
	assert_eq!(entries.len(), 1);
	assert_eq!(entries[0]["seq"], 2);
}

#[tokio::test]
async fn a_filter_reports_when_it_stopped_scanning() {
	let (application, _directory) = app_with_scan_pages(1).await;
	let signer = key(5);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let entry = feed_wire(&signer, &project_id, 1, None, release_digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);

	let request = axum::http::Request::get(format!("/v1/projects/{project_id}/feed?limit=1&game_version=9.9.9"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	let page = body_json(response).await;

	assert!(page["entries"].as_array().expect("entries").is_empty());
	assert_eq!(page["truncated"], true);
	assert_eq!(page["next"], 1);
}

#[tokio::test]
async fn a_feed_can_be_filtered_by_loader() {
	let (application, _directory) = app().await;
	let signer = key(6);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let entry = feed_wire(&signer, &project_id, 1, None, release_digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let previous = id_bytes(body_json(response).await["entry"].as_str().expect("entry id"));

	let (release, digest) = release_wire_no_loader(&signer, &project_id, 0x81, "2.0.0");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(release))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);
	let entry = feed_wire(&signer, &project_id, 2, Some(previous), digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);

	let request = axum::http::Request::get(format!("/v1/projects/{project_id}/feed?loader={}", sample_id("fabric")))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	let page = body_json(response).await;
	let entries = page["entries"].as_array().expect("entries");
	assert_eq!(entries.len(), 1);
	assert_eq!(entries[0]["seq"], 1);
}

#[tokio::test]
async fn a_feed_can_be_filtered_by_loader_version() {
	let (application, _directory) = app().await;
	let signer = key(7);
	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire(&signer, PROJECT_KINDS)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let project_id = body_json(response).await["project_id"]
		.as_str()
		.expect("project id")
		.to_string();

	let (older, older_digest) =
		release_wire_for_game_with_loader(&signer, &project_id, 0x90, "1.0.0", "1.20.1", Some("fabric"), Some("0.14.0"));
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(older))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);
	let entry = feed_wire(&signer, &project_id, 1, None, older_digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let previous = id_bytes(body_json(response).await["entry"].as_str().expect("entry id"));

	let (newer, newer_digest) =
		release_wire_for_game_with_loader(&signer, &project_id, 0x91, "2.0.0", "1.20.1", Some("fabric"), Some("0.15.0"));
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(newer))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);
	let entry = feed_wire(&signer, &project_id, 2, Some(previous), newer_digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);

	let loader = sample_id("fabric");
	let request = axum::http::Request::get(format!(
		"/v1/projects/{project_id}/feed?loader={loader}&loader_version=0.15.0"
	))
	.body(Body::empty())
	.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let page = body_json(response).await;
	let entries = page["entries"].as_array().expect("entries");
	assert_eq!(entries.len(), 1);
	assert_eq!(entries[0]["seq"], 2);

	let request = axum::http::Request::get(format!("/v1/projects/{project_id}/feed?loader={loader}&loader_version=9.9.9"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	let page = body_json(response).await;
	assert!(page["entries"].as_array().expect("entries").is_empty());
}

#[tokio::test]
async fn feed_titles_a_withdrawal() {
	let (application, _directory) = app().await;
	let signer = key(13);
	let (project_id, _release_digest) = publish_project(&application, &signer).await;

	let withdrawal = Withdrawal {
		protocol: 1,
		release_id: "gd:sha256:whatever".to_string(),
		reason: "compromise".to_string(),
		note: None,
		declared_time: 1_760_000_200,
	};
	let signed = sign_payload(Kind::Release, &withdrawal, &[&signer]);
	let digest = object_id(Kind::Release, &signed.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	application.clone().oneshot(request).await.expect("response");

	let entry = feed_wire_kind(&signer, &project_id, 1, None, digest, "release-withdrawn");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	application.clone().oneshot(request).await.expect("response");

	let request = axum::http::Request::get(format!("/v1/projects/{project_id}/feed"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["entries"][0]["title"], "withdrawn: compromise");
}
