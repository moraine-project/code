use axum::Router;
use axum::body::Body;
use moraine_crypto::{ObjectKind as Kind, SigningKey, object_id};
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use super::auth::body_json;
use super::wire::{PROJECT_KINDS, feed_wire_kind, genesis_wire, release_wire, sample_id};

pub(crate) async fn publish_profile(
	application: &Router,
	signer: &SigningKey,
	project_id: &str,
	display_name: &str,
) -> String {
	publish_profile_for_game(application, signer, project_id, display_name, "minecraft").await
}

pub(crate) async fn publish_profile_for_game(
	application: &Router,
	signer: &SigningKey,
	project_id: &str,
	display_name: &str,
	game_id: &str,
) -> String {
	use moraine_model::profile::ProfileRevision;

	let profile = ProfileRevision {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: sample_id(game_id),
		revision_nonce: vec![0x41; 16],
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
	let signed = sign_payload(Kind::Profile, &profile, &[signer]);
	let digest = object_id(Kind::Profile, &signed.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/profile"))
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::CREATED);
	let feed = feed_wire_kind(signer, project_id, 1, None, digest, "profile-updated");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(feed))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::CREATED);
	format!("gd:sha256:{}", hex::encode(digest))
}

pub(crate) fn id_bytes(id: &str) -> [u8; 32] {
	let hex = id.strip_prefix("gd:sha256:").expect("object id");
	<[u8; 32]>::try_from(hex::decode(hex).expect("hex").as_slice()).expect("digest")
}

pub(crate) async fn publish_project(application: &Router, signer: &SigningKey) -> (String, [u8; 32]) {
	publish_project_with_kinds(application, signer, PROJECT_KINDS).await
}

pub(crate) async fn store_release(application: &Router, project_id: &str, wire: Vec<u8>) {
	let path = format!("/v1/projects/{project_id}/objects/release");
	let request = axum::http::Request::post(&path).body(Body::from(wire)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), axum::http::StatusCode::CREATED);
}

pub(crate) async fn publish_project_with_kinds(
	application: &Router,
	signer: &SigningKey,
	kinds: &[&str],
) -> (String, [u8; 32]) {
	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire(signer, kinds)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let receipt = body_json(response).await;
	let project_id = receipt["project_id"].as_str().expect("project id").to_string();
	let (release, release_digest) = release_wire(signer, &project_id);
	let path = format!("/v1/projects/{project_id}/objects/release");
	let request = axum::http::Request::post(&path).body(Body::from(release)).expect("request");
	application.clone().oneshot(request).await.expect("response");
	(project_id, release_digest)
}
