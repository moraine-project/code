use axum::body::Body;
use axum::http::StatusCode;
use moraine_crypto::{ObjectKind as Kind, object_id};
use moraine_model::profile::ProfileRevision;
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use crate::test_support::*;

async fn publish_revision(
	application: &axum::Router,
	signer: &moraine_crypto::SigningKey,
	project_id: &str,
	sequence: u64,
	previous: Option<[u8; 32]>,
	nonce: u8,
	display_name: &str,
) -> [u8; 32] {
	let profile = ProfileRevision {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: sample_id("minecraft"),
		revision_nonce: vec![nonce; 16],
		display_name: display_name.to_string(),
		summary: "A summary".to_string(),
		description: "A description".to_string(),
		icon: None,
		gallery: Vec::new(),
		links: Vec::new(),
		communities: Vec::new(),
		categories: Vec::new(),
		tags: Vec::new(),
		rights: None,
		declared_time: 1_760_000_000,
	};
	let signed = sign_payload(Kind::Profile, &profile, &[signer]).expect("valid signed profile");
	let digest = object_id(Kind::Profile, &signed.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/profile"))
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);
	let entry = feed_wire_kind(signer, project_id, sequence, previous, digest, "profile-updated");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	id_bytes(body_json(response).await["entry"].as_str().expect("entry"))
}

#[tokio::test]
async fn serves_a_historical_profile_revision() {
	let (application, _directory) = app().await;
	let signer = key(30);
	let (project_id, _release) = publish_project(&application, &signer).await;
	let first = publish_revision(&application, &signer, &project_id, 1, None, 0x51, "First Name").await;
	publish_revision(&application, &signer, &project_id, 2, Some(first), 0x52, "Second Name").await;

	let current = axum::http::Request::get(format!("/v1/projects/{project_id}/profile"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(current).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	assert_eq!(body_json(response).await["display_name"], "Second Name");

	let at_first = axum::http::Request::get(format!("/v1/projects/{project_id}/profile?at=1"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(at_first).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["display_name"], "First Name");

	let at_second = axum::http::Request::get(format!("/v1/projects/{project_id}/profile?at=2"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(at_second).await.expect("response");
	assert_eq!(body_json(response).await["display_name"], "Second Name");

	let before_any = axum::http::Request::get(format!("/v1/projects/{project_id}/profile?at=0"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(before_any).await.expect("response");
	assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
