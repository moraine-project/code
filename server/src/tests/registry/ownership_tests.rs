use axum::body::Body;
use axum::http::StatusCode;
use moraine_crypto::{ObjectKind as Kind, object_id};
use moraine_model::delegation::{Delegation, KeyDelegation, OwnerRef, OwnershipTransfer};
use moraine_model::genesis::RootKey;
use moraine_model::release::Withdrawal;
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use crate::test_support::*;

#[tokio::test]
async fn accepts_a_delegated_release_key_and_rejects_an_unrelated_one() {
	let (application, _directory) = app().await;
	let root = key(1);
	let delegated = key(2);
	let intruder = key(3);
	let kinds = ["delegation", "release", "profile", "feed-entry"];
	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire(&root, &kinds)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let receipt = body_json(response).await;
	let project_id = receipt["project_id"].as_str().expect("project id").to_string();

	let delegation = Delegation::Key(KeyDelegation {
		protocol: 1,
		project_id: project_id.clone(),
		delegate_key: RootKey::from_public_key(delegated.verifying_key().to_bytes().to_vec()).expect("delegate"),
		allowed_kinds: vec!["release".to_string(), "feed-entry".to_string()],
		channels: None,
		max_version_scope: None,
		valid_from_seq: None,
		expires_at: None,
		issued_at: 1_760_000_000,
		previous_delegation_digest: None,
	});
	let wire = sign_payload(Kind::Delegation, &delegation, &[&root])
		.expect("valid signed delegation")
		.wire_bytes();
	let path = format!("/v1/projects/{project_id}/objects/delegation");
	let request = axum::http::Request::post(&path).body(Body::from(wire)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let (release, release_digest) = release_wire(&delegated, &project_id);
	let path = format!("/v1/projects/{project_id}/objects/release");
	let request = axum::http::Request::post(&path).body(Body::from(release)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let feed = feed_wire(&delegated, &project_id, 1, None, release_digest);
	let path = format!("/v1/projects/{project_id}/feed");
	let request = axum::http::Request::post(&path).body(Body::from(feed)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let (forged, _) = release_wire(&intruder, &project_id);
	let path = format!("/v1/projects/{project_id}/objects/release");
	let request = axum::http::Request::post(&path).body(Body::from(forged)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);

	let request = axum::http::Request::get("/metrics").body(Body::empty()).expect("request");
	let response = application.oneshot(request).await.expect("response");
	let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.expect("body");
	let text = String::from_utf8(body.to_vec()).expect("utf8");
	assert!(text.contains("moraine_signature_failures_total 1"), "{text}");
}

#[tokio::test]
async fn ownership_transfer_requires_two_signatures_and_updates_the_owner() {
	let (application, _directory) = app().await;
	let first = key(10);
	let second = key(11);
	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire_roots(&[&first, &second], PROJECT_KINDS)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let receipt = body_json(response).await;
	let project_id = receipt["project_id"].as_str().expect("project id").to_string();

	let transfer = Delegation::OwnershipTransfer(OwnershipTransfer {
		protocol: 1,
		project_id: project_id.clone(),
		from_owner: OwnerRef {
			kind: "user".to_string(),
			id: "user-a".to_string(),
			key_id: first.key_id(),
		},
		to_owner: OwnerRef {
			kind: "org".to_string(),
			id: "org-b".to_string(),
			key_id: second.key_id(),
		},
		issued_at: 1_760_000_000,
		previous_delegation_digest: None,
	});
	let signed_transfer = sign_payload(Kind::Delegation, &transfer, &[&first, &second]).expect("valid signed transfer");
	let transfer_digest = object_id(Kind::Delegation, &signed_transfer.payload_bytes);
	let response = application
		.clone()
		.oneshot(
			axum::http::Request::post(format!("/v1/projects/{project_id}/transfer"))
				.body(Body::from(signed_transfer.wire_bytes()))
				.expect("request"),
		)
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let entry = feed_wire_kind(&first, &project_id, 1, None, transfer_digest, "ownership-transferred");
	let response = application
		.clone()
		.oneshot(
			axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
				.body(Body::from(entry))
				.expect("request"),
		)
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let summary = body_json(
		application
			.clone()
			.oneshot(
				axum::http::Request::get(format!("/v1/projects/{project_id}"))
					.body(Body::empty())
					.expect("request"),
			)
			.await
			.expect("response"),
	)
	.await;
	assert_eq!(summary["owner"]["id"], "org-b");

	let one_signature = Delegation::OwnershipTransfer(OwnershipTransfer {
		protocol: 1,
		project_id: project_id.clone(),
		from_owner: OwnerRef {
			kind: "org".to_string(),
			id: "org-b".to_string(),
			key_id: first.key_id(),
		},
		to_owner: OwnerRef {
			kind: "user".to_string(),
			id: "user-c".to_string(),
			key_id: second.key_id(),
		},
		issued_at: 1_760_000_001,
		previous_delegation_digest: None,
	});
	let wire = sign_payload(Kind::Delegation, &one_signature, &[&first])
		.expect("valid signed delegation")
		.wire_bytes();
	let response = application
		.clone()
		.oneshot(
			axum::http::Request::post(format!("/v1/projects/{project_id}/transfer"))
				.body(Body::from(wire))
				.expect("request"),
		)
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);

	let stranger = key(99);
	let mismatched = Delegation::OwnershipTransfer(OwnershipTransfer {
		protocol: 1,
		project_id: project_id.clone(),
		from_owner: OwnerRef {
			kind: "user".to_string(),
			id: "user-a".to_string(),
			key_id: stranger.key_id(),
		},
		to_owner: OwnerRef {
			kind: "org".to_string(),
			id: "org-b".to_string(),
			key_id: first.key_id(),
		},
		issued_at: 1_760_000_002,
		previous_delegation_digest: None,
	});
	let wire = sign_payload(Kind::Delegation, &mismatched, &[&first, &second])
		.expect("valid signed delegation")
		.wire_bytes();
	let response = application
		.clone()
		.oneshot(
			axum::http::Request::post(format!("/v1/projects/{project_id}/transfer"))
				.body(Body::from(wire))
				.expect("request"),
		)
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn withdrawal_marks_a_release_without_rewriting_it() {
	let (application, _directory) = app().await;
	let signer = key(12);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let release_id = format!("gd:sha256:{}", hex::encode(release_digest));

	let withdrawal = Withdrawal {
		protocol: 1,
		project_id: project_id.clone(),
		release_id: release_id.clone(),
		reason: "compromise".to_string(),
		note: Some("automated key leak".to_string()),
		declared_time: 1_760_000_100,
	};
	let signed = sign_payload(Kind::Release, &withdrawal, &[&signer]).expect("valid signed withdrawal");
	let digest = object_id(Kind::Release, &signed.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let entry = feed_wire_kind(&signer, &project_id, 1, None, digest, "release-withdrawn");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let request = axum::http::Request::get(format!("/v1/projects/{project_id}/releases/{}", hex::encode(release_digest)))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	let view = body_json(response).await;
	assert_eq!(view["withdrawal"]["reason"], "compromise");
	assert_eq!(view["withdrawal"]["note"], "automated key leak");
	assert_eq!(view["human_version"], "1.0.0");
}
