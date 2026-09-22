use axum::body::Body;
use axum::http::StatusCode;
use moraine_crypto::{ObjectKind as Kind, SigningKey, object_id};
use moraine_model::delegation::{Delegation, RecoveryEvent, ReleaseWindow};
use moraine_model::genesis::RootKey;
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use crate::test_support::*;

#[tokio::test]
async fn recovery_replaces_the_root_set_and_revokes_the_old_key() {
	let (application, _directory) = app().await;
	let compromised = SigningKey::from_seed(&[0xA1; 32]);
	let replacement = SigningKey::from_seed(&[0xB2; 32]);
	let competing = SigningKey::from_seed(&[0xC3; 32]);
	let (project_id, _release) = publish_project(&application, &compromised).await;

	let event = Delegation::Recovery(RecoveryEvent {
		protocol: 1,
		project_id: project_id.clone(),
		compromised_key_ids: vec![compromised.key_id()],
		valid_from_seq: 2,
		replacement_roots: vec![RootKey::from_public_key(replacement.verifying_key().to_bytes().to_vec()).expect("root")],
		affected_release_window: ReleaseWindow { from_seq: 0, to_seq: 1 },
		reason: "test key compromise".to_string(),
		declared_time: 1_760_000_000,
	});
	let signed = sign_payload(Kind::Delegation, &event, &[&compromised]);
	let digest = object_id(Kind::Delegation, &signed.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/delegation"))
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"{}",
		String::from_utf8_lossy(&axum::body::to_bytes(response.into_body(), 4096).await.unwrap())
	);

	let rival = Delegation::Recovery(RecoveryEvent {
		protocol: 1,
		project_id: project_id.clone(),
		compromised_key_ids: vec![compromised.key_id()],
		valid_from_seq: 2,
		replacement_roots: vec![RootKey::from_public_key(competing.verifying_key().to_bytes().to_vec()).expect("root")],
		affected_release_window: ReleaseWindow { from_seq: 0, to_seq: 1 },
		reason: "a second claim at the same sequence".to_string(),
		declared_time: 1_760_000_000,
	});
	let signed_rival = sign_payload(Kind::Delegation, &rival, &[&compromised]);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/delegation"))
		.body(Body::from(signed_rival.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let entry = feed_wire_kind(&compromised, &project_id, 1, None, digest, "recovery");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"{}",
		String::from_utf8_lossy(&axum::body::to_bytes(response.into_body(), 4096).await.unwrap())
	);

	let view = axum::http::Request::get(format!("/v1/projects/{project_id}/recovery"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(view).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["recovered"], true);
	assert_eq!(view["threshold"], 1);
	assert_eq!(
		view["roots"][0]["public_key"],
		hex::encode(replacement.verifying_key().to_bytes())
	);
	let claims = view["claims"].as_array().expect("claims");
	assert_eq!(claims.len(), 2, "a competing claim is surfaced, not hidden");
	assert_eq!(claims.iter().filter(|claim| claim["applied"] == true).count(), 1);
	assert_eq!(claims.iter().filter(|claim| claim["applied"] == false).count(), 1);

	let compromised_release = release_wire_variant(&compromised, &project_id, 0x77, "9.9.9").0;
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(compromised_release))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(
		response.status(),
		StatusCode::BAD_REQUEST,
		"the compromised key can no longer authorize anything"
	);

	let replacement_release = release_wire_variant(&replacement, &project_id, 0x78, "9.9.9").0;
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(replacement_release))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED, "the replacement root can publish");
}
