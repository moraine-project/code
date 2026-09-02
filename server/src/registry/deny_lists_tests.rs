use axum::body::Body;
use axum::http::{StatusCode, header};
use moraine_crypto::{ObjectKind as Kind, SigningKey};
use moraine_model::deny_list::{DenyList, DenyListEntry, DenyTarget};
use moraine_model::moderation::ScopeKind;
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use crate::test_support::*;

fn entry(target_kind: DenyTarget, target_id: &str, reason: &str, valid_until: Option<i64>) -> DenyListEntry {
	DenyListEntry {
		target_kind,
		target_id: target_id.to_string(),
		reason_code: reason.to_string(),
		reason_taxonomy_version: 1,
		scope_kind: ScopeKind::Instance,
		scope_id: "dir.example".to_string(),
		valid_from: None,
		valid_until,
	}
}

async fn pin(application: &axum::Router, issuer: &SigningKey) {
	let (session, csrf) = login(application, "issuer@example.org").await;
	let cookie = format!("moraine_session={session}; moraine_csrf={csrf}");
	let request = axum::http::Request::post("/v1/providers/dir.example/keys")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, &cookie)
		.header("x-csrf-token", &csrf)
		.body(Body::from(
			serde_json::json!({ "public_key": hex::encode(issuer.verifying_key().to_bytes()) }).to_string(),
		))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn records_a_subscribed_deny_list_and_annotates_search() {
	let (application, _directory) = app().await;
	let issuer = SigningKey::from_seed(&[0xE1; 32]);
	pin(&application, &issuer).await;
	let signer = key(44);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	publish_profile(&application, &signer, &project_id, "Sketchy Mod").await;

	let list = DenyList {
		protocol: 1,
		issuer_id: "dir.example".to_string(),
		entries: vec![
			entry(DenyTarget::Project, &project_id, "malware-confirmed", None),
			entry(
				DenyTarget::ArtifactDigest,
				&hex::encode(release_digest),
				"malware-suspected",
				None,
			),
			entry(DenyTarget::Project, "gd:sha256:ff", "spam", Some(1_700_000_000)),
		],
		issued_at: 1_760_000_000,
	};
	let signed = sign_payload(Kind::DenyList, &list, &[&issuer]);
	let request = axum::http::Request::post("/v1/deny-lists")
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let entries = axum::http::Request::get(format!("/v1/deny-lists?project={project_id}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(entries).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let entries = body_json(response).await;
	assert_eq!(entries.as_array().expect("entries").len(), 1);
	assert_eq!(entries[0]["reason_code"], "malware-confirmed");
	assert_eq!(entries[0]["issuer_id"], "dir.example");

	let expired = axum::http::Request::get("/v1/deny-lists?project=gd:sha256:ff")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(expired).await.expect("response");
	assert!(
		body_json(response).await.as_array().expect("entries").is_empty(),
		"an expired entry stops applying"
	);

	let search = axum::http::Request::get("/v1/search?q=sketchy")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(search).await.expect("response");
	let page = body_json(response).await;
	let annotations = page["results"][0]["annotations"].as_array().expect("annotations");
	assert!(
		annotations.iter().any(|entry| entry["kind"] == "deny-list"),
		"a subscribed finding is surfaced: {annotations:?}"
	);

	let wrong_key = SigningKey::from_seed(&[0xE2; 32]);
	let signed = sign_payload(Kind::DenyList, &list, &[&wrong_key]);
	let request = axum::http::Request::post("/v1/deny-lists")
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);

	let unpinned = DenyList {
		issuer_id: "nobody.example".to_string(),
		..list
	};
	let signed = sign_payload(Kind::DenyList, &unpinned, &[&issuer]);
	let request = axum::http::Request::post("/v1/deny-lists")
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CONFLICT);
}
