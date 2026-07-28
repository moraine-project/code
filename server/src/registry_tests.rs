use axum::body::Body;
use axum::http::header;
use moraine_crypto::{ObjectKind as Kind, object_id};
use moraine_model::advisory::{Advisory, Affected, Category, Severity};
use moraine_model::compatibility::Side;
use moraine_model::delegation::{Delegation, KeyDelegation, OwnerRef, OwnershipTransfer};
use moraine_model::dependency::TargetKind;
use moraine_model::genesis::RootKey;
use moraine_model::modpack::{ModpackEntry, ModpackManifest, ModpackOverride};
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
	let wire = sign_payload(Kind::Delegation, &delegation, &[&root]).wire_bytes();
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
	let older = axum::http::Request::get(format!("/v1/submissions?before={oldest}"))
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

#[tokio::test]
async fn serves_json_views_of_profile_and_release() {
	use moraine_model::profile::ProfileRevision;

	let (application, _directory) = app().await;
	let signer = key(7);
	let (project_id, release_digest) = publish_project(&application, &signer).await;

	let profile = ProfileRevision {
		protocol: 1,
		project_id: project_id.clone(),
		game_id: sample_id("minecraft"),
		revision_nonce: vec![0x24; 16],
		display_name: "Example Mod".to_string(),
		summary: "A worked example".to_string(),
		description: "Longer description".to_string(),
		icon: None,
		gallery: Vec::new(),
		links: Vec::new(),
		communities: Vec::new(),
		categories: vec!["utility".to_string()],
		tags: vec!["client".to_string()],
		rights: None,
		declared_time: 1_760_000_000,
	};
	let signed_profile = sign_payload(Kind::Profile, &profile, &[&signer]);
	let profile_digest = object_id(Kind::Profile, &signed_profile.payload_bytes);
	let path = format!("/v1/projects/{project_id}/objects/profile");
	let request = axum::http::Request::post(&path)
		.body(Body::from(signed_profile.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let entry = FeedEntry {
		protocol: 1,
		project_id: project_id.clone(),
		sequence: 1,
		previous: None,
		kind: "profile-updated".to_string(),
		object_digest: profile_digest.to_vec(),
		declared_at: 1_760_000_000,
	};
	let feed = sign_payload(Kind::FeedEntry, &entry, &[&signer]).wire_bytes();
	let path = format!("/v1/projects/{project_id}/feed");
	let request = axum::http::Request::post(&path).body(Body::from(feed)).expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let profile_request = axum::http::Request::get(format!("/v1/projects/{project_id}/profile"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(profile_request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["display_name"], "Example Mod");
	assert_eq!(view["tags"][0], "client");

	let release_request =
		axum::http::Request::get(format!("/v1/projects/{project_id}/releases/{}", hex::encode(release_digest)))
			.body(Body::empty())
			.expect("request");
	let response = application.clone().oneshot(release_request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["human_version"], "1.0.0");
	assert_eq!(view["artifacts"][0]["is_primary"], true);

	let search = axum::http::Request::get("/v1/search?q=example")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(search).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let page = body_json(response).await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);
	assert_eq!(page["results"][0]["display_name"], "Example Mod");

	let tagged = axum::http::Request::get("/v1/search?tag=client")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(tagged).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);

	let missing = axum::http::Request::get("/v1/search?tag=server")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(missing).await.expect("response");
	let page = body_json(response).await;
	assert!(page["results"].as_array().expect("results").is_empty());

	let by_loader = axum::http::Request::get(format!("/v1/search?loader={}", sample_id("fabric")))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(by_loader).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);

	let wrong_loader = axum::http::Request::get(format!("/v1/search?loader={}", sample_id("forge")))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(wrong_loader).await.expect("response");
	let page = body_json(response).await;
	assert!(page["results"].as_array().expect("results").is_empty());

	let by_name = axum::http::Request::get("/v1/search?sort=name")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(by_name).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let unsupported = axum::http::Request::get("/v1/search?sort=popularity")
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(unsupported).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn looks_up_a_release_from_an_artifact_digest() {
	let (application, _directory) = app().await;
	let signer = key(8);
	let (project_id, release_digest) = publish_project(&application, &signer).await;

	let digest = hex::encode([0xABu8; 32]);
	let request = axum::http::Request::get(format!("/v1/lookup?sha256={digest}"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["matches"].as_array().expect("matches").len(), 1);
	assert_eq!(view["matches"][0]["project_id"], project_id);
	assert_eq!(
		view["matches"][0]["release"],
		format!("gd:sha256:{}", hex::encode(release_digest))
	);
	assert_eq!(view["matches"][0]["human_version"], "1.0.0");
	assert_eq!(view["matches"][0]["filename"], "example.jar");

	let unknown = axum::http::Request::get(format!("/v1/lookup?sha256={}", hex::encode([9u8; 32])))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(unknown).await.expect("response");
	let view = body_json(response).await;
	assert!(view["matches"].as_array().expect("matches").is_empty());
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
		},
		to_owner: OwnerRef {
			kind: "org".to_string(),
			id: "org-b".to_string(),
		},
		issued_at: 1_760_000_000,
		previous_delegation_digest: None,
	});
	let signed_transfer = sign_payload(Kind::Delegation, &transfer, &[&first, &second]);
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
		},
		to_owner: OwnerRef {
			kind: "user".to_string(),
			id: "user-c".to_string(),
		},
		issued_at: 1_760_000_001,
		previous_delegation_digest: None,
	});
	let wire = sign_payload(Kind::Delegation, &one_signature, &[&first]).wire_bytes();
	let response = application
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
		release_id: release_id.clone(),
		reason: "compromise".to_string(),
		note: Some("automated key leak".to_string()),
		declared_time: 1_760_000_100,
	};
	let signed = sign_payload(Kind::Release, &withdrawal, &[&signer]);
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

#[tokio::test]
async fn release_view_lists_pinned_provider_advisories() {
	let (application, _directory) = app().await;
	let signer = key(14);
	let provider = key(15);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let (session, csrf) = login(&application, "provider@example.org").await;
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

	let advisory = Advisory {
		protocol: 1,
		provider_id: "scanner".to_string(),
		project_id: project_id.clone(),
		game_id: sample_id("minecraft"),
		affected: Affected {
			digest: Some(vec![0xAB; 32]),
			predicate: None,
		},
		severity: Severity::High,
		category: Category::Malware,
		taxonomy_version: 1,
		block_promotion: true,
		evidence_ref: None,
		published_at: 1_760_000_300,
		expires_at: None,
		retracted_at: None,
	};
	let signed = sign_payload(Kind::Advisory, &advisory, &[&provider]);
	let request = axum::http::Request::post("/v1/advisories")
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let request = axum::http::Request::get(format!("/v1/projects/{project_id}/releases/{}", hex::encode(release_digest)))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	let view = body_json(response).await;
	assert_eq!(view["advisories"].as_array().expect("advisories").len(), 1);
	assert_eq!(view["advisories"][0]["provider_id"], "scanner");
	assert_eq!(view["advisories"][0]["block_promotion"], true);

	let unknown = Advisory {
		provider_id: "ghost".to_string(),
		..advisory
	};
	let signed = sign_payload(Kind::Advisory, &unknown, &[&provider]);
	let request = axum::http::Request::post("/v1/advisories")
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn serves_a_modpack_manifest_and_rejects_an_escaping_override() {
	let (application, _directory) = app().await;
	let signer = key(17);
	let (project_id, release_digest) = publish_project(&application, &signer).await;

	let manifest = ModpackManifest {
		protocol: 1,
		project_id: project_id.clone(),
		game_id: sample_id("minecraft"),
		loader_id: None,
		entries: vec![ModpackEntry {
			ordinal: 0,
			target_kind: TargetKind::Project,
			target_id: project_id.clone(),
			release_id: format!("gd:sha256:{}", hex::encode(release_digest)),
			digest: release_digest.to_vec(),
			applies_to: Side::Both,
		}],
		overrides: vec![ModpackOverride {
			digest: vec![0x77; 32],
			target_path: "config/example.toml".to_string(),
			applies_to: Side::Client,
		}],
		server_manifest_digest: None,
		declared_time: 1_760_000_500,
	};
	let signed = sign_payload(Kind::Modpack, &manifest, &[&signer]);
	let pack_digest = object_id(Kind::Modpack, &signed.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/modpack"))
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let request = axum::http::Request::get(format!("/v1/packs/{}", hex::encode(pack_digest)))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["payload"]["entries"].as_array().expect("entries").len(), 1);
	assert_eq!(view["payload"]["overrides"][0]["target_path"], "config/example.toml");

	let mut escaping = manifest.clone();
	escaping.overrides[0].target_path = "../escape.txt".to_string();
	let signed = sign_payload(Kind::Modpack, &escaping, &[&signer]);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/modpack"))
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn object_documents_support_range_and_head() {
	let (application, _directory) = app().await;
	let signer = key(18);
	let (_project_id, release_digest) = publish_project(&application, &signer).await;
	let path = format!("/v1/objects/{}", hex::encode(release_digest));

	let head = axum::http::Request::head(&path).body(Body::empty()).expect("request");
	let response = application.clone().oneshot(head).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	assert!(response.headers().get(header::CONTENT_LENGTH).is_some());

	let ranged = axum::http::Request::get(&path)
		.header(header::RANGE, "bytes=0-3")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(ranged).await.expect("response");
	assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
	let content_range = response.headers()[header::CONTENT_RANGE].to_str().expect("range");
	assert!(content_range.starts_with("bytes 0-3/"));

	let beyond = axum::http::Request::get(&path)
		.header(header::RANGE, "bytes=99999999-")
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(beyond).await.expect("response");
	assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
}
