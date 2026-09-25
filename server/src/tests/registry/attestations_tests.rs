use axum::body::Body;
use axum::http::{StatusCode, header};
use moraine_crypto::{ObjectKind as Kind, SigningKey};
use moraine_model::attestation::{Attestation, AttestationKind, AttestationObject};
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use crate::test_support::{app, body_json, login, publish_project_with_kinds};

fn attestation_wire(
	provider: &SigningKey,
	signer_id: &str,
	subject_id: &str,
	artifact_digest: [u8; 32],
	kind: AttestationKind,
) -> Vec<u8> {
	let attestation = Attestation {
		protocol: 1,
		artifact_digest: artifact_digest.to_vec(),
		subject_kind: "project".to_string(),
		subject_id: subject_id.to_string(),
		kind,
		media_type: "application/spdx+json".to_string(),
		body_digest: Some(vec![0x33; 32]),
		body_inline: None,
		signer_id: signer_id.to_string(),
		issued_at: 1_760_000_000,
	};
	sign_payload(Kind::Attestation, &AttestationObject::Evidence(attestation), &[provider])
		.expect("valid signed attestation")
		.wire_bytes()
}

async fn pin_provider(application: &axum::Router, provider: &SigningKey) {
	let (session, csrf) = login(application, "provider@example.org").await;
	let cookie = format!("moraine_session={session}; moraine_csrf={csrf}");
	let pin = axum::http::Request::post("/v1/providers/scanner/keys")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, &cookie)
		.header("x-csrf-token", &csrf)
		.body(Body::from(
			serde_json::json!({ "public_key": hex::encode(provider.verifying_key().to_bytes()) }).to_string(),
		))
		.expect("request");
	let response = application.clone().oneshot(pin).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn publishes_and_lists_evidence_attestations() {
	let (application, _directory) = app().await;
	let provider = SigningKey::from_seed(&[0x91; 32]);
	pin_provider(&application, &provider).await;
	let artifact = [0xAB; 32];

	let publish = axum::http::Request::post("/v1/attestations")
		.body(Body::from(attestation_wire(
			&provider,
			"scanner",
			"gd:sha256:aa",
			artifact,
			AttestationKind::Sbom,
		)))
		.expect("request");
	let response = application.clone().oneshot(publish).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let list = axum::http::Request::get(format!("/v1/attestations/{}", hex::encode(artifact)))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(list).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let attestations = body_json(response).await;
	assert_eq!(attestations.as_array().expect("attestations").len(), 1);
	assert_eq!(attestations[0]["kind"], "sbom");
	assert_eq!(attestations[0]["signer_id"], "scanner");
	assert_eq!(attestations[0]["has_inline_body"], false);
	assert!(attestations[0]["body_digest"].as_str().is_some());

	let filtered = axum::http::Request::get(format!("/v1/attestations/{}?kind=review", hex::encode(artifact)))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(filtered).await.expect("response");
	assert!(body_json(response).await.as_array().expect("attestations").is_empty());

	let bad_kind = axum::http::Request::get(format!("/v1/attestations/{}?kind=nonsense", hex::encode(artifact)))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(bad_kind).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);

	let unpinned = SigningKey::from_seed(&[0x92; 32]);
	let publish = axum::http::Request::post("/v1/attestations")
		.body(Body::from(attestation_wire(
			&unpinned,
			"ghost",
			"gd:sha256:aa",
			artifact,
			AttestationKind::Sbom,
		)))
		.expect("request");
	let response = application.oneshot(publish).await.expect("response");
	assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn indexes_a_project_signed_attestation() {
	let (application, _directory) = app().await;
	let signer = SigningKey::from_seed(&[0x93; 32]);
	let (project_id, _release) =
		publish_project_with_kinds(&application, &signer, &["delegation", "release", "profile", "attestation"]).await;
	let artifact = [0xAB; 32];

	let publish = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/attestation"))
		.body(Body::from(attestation_wire(
			&signer,
			&project_id,
			&project_id,
			artifact,
			AttestationKind::BuildProvenance,
		)))
		.expect("request");
	let response = application.clone().oneshot(publish).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let list = axum::http::Request::get(format!("/v1/attestations/{}", hex::encode(artifact)))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(list).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let attestations = body_json(response).await;
	assert_eq!(attestations.as_array().expect("attestations").len(), 1);
	assert_eq!(attestations[0]["kind"], "build-provenance");

	let release_view = axum::http::Request::get(format!("/v1/projects/{project_id}/releases/{}", hex::encode(_release)))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(release_view).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	let evidence = view["attestations"].as_array().expect("attestations");
	assert_eq!(evidence.len(), 1);
	assert_eq!(evidence[0]["kind"], "build-provenance");
	assert_eq!(evidence[0]["signer_id"], project_id);
	assert!(
		!view["compatibility"].as_array().expect("compatibility").is_empty(),
		"declared compatibility stays separate from evidence"
	);
}
