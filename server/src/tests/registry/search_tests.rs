use axum::body::Body;
use axum::http::StatusCode;
use moraine_crypto::{ObjectKind as Kind, object_id};
use moraine_model::feed::FeedEntry;
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use crate::test_support::*;

#[tokio::test]
async fn ranks_a_name_match_above_a_description_match() {
	use moraine_model::profile::ProfileRevision;

	let (application, _directory) = app().await;
	for (signer_byte, name, description, nonce) in [
		(8u8, "Widget", "An unrelated description", 0x31u8),
		(9u8, "Gadget", "A widget in the description", 0x32u8),
	] {
		let signer = key(signer_byte);
		let (project_id, release_digest) = publish_project(&application, &signer).await;
		let profile = ProfileRevision {
			protocol: 1,
			project_id: project_id.clone(),
			game_id: sample_id("minecraft"),
			revision_nonce: vec![nonce; 16],
			display_name: name.to_string(),
			summary: "Summary".to_string(),
			description: description.to_string(),
			icon: None,
			gallery: Vec::new(),
			links: Vec::new(),
			communities: Vec::new(),
			categories: Vec::new(),
			tags: Vec::new(),
			rights: None,
			declared_time: 1_760_000_000,
		};
		let signed = sign_payload(Kind::Profile, &profile, &[&signer]);
		let digest = object_id(Kind::Profile, &signed.payload_bytes);
		let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/profile"))
			.body(Body::from(signed.wire_bytes()))
			.expect("request");
		assert_eq!(
			application.clone().oneshot(request).await.expect("response").status(),
			StatusCode::CREATED
		);
		let entry = FeedEntry {
			protocol: 1,
			project_id: project_id.clone(),
			sequence: 1,
			previous: None,
			kind: "profile-updated".to_string(),
			object_digest: digest.to_vec(),
			declared_at: 1_760_000_000,
		};
		let feed = sign_payload(Kind::FeedEntry, &entry, &[&signer]).wire_bytes();
		let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
			.body(Body::from(feed))
			.expect("request");
		assert_eq!(
			application.clone().oneshot(request).await.expect("response").status(),
			StatusCode::CREATED
		);
		let _ = release_digest;
	}

	let request = axum::http::Request::get("/v1/search?q=widget")
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	let status = response.status();
	let bytes = axum::body::to_bytes(response.into_body(), 65536).await.expect("body");
	assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
	let page: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
	let results = page["results"].as_array().expect("results");
	assert_eq!(results.len(), 2);
	assert_eq!(results[0]["display_name"], "Widget");
}

#[tokio::test]
async fn finds_a_project_by_a_phrase_from_its_changelog() {
	use moraine_model::changelog::{Changelog, ChangelogSection, LocaleSection};
	use moraine_model::profile::ProfileRevision;

	let (application, _directory) = app().await;
	let signer = key(10);
	let (project_id, _release) =
		publish_project_with_kinds(&application, &signer, &["delegation", "release", "profile", "changelog"]).await;
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
		categories: Vec::new(),
		tags: Vec::new(),
		rights: None,
		declared_time: 1_760_000_000,
	};
	let signed_profile = sign_payload(Kind::Profile, &profile, &[&signer]);
	let profile_digest = object_id(Kind::Profile, &signed_profile.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/profile"))
		.body(Body::from(signed_profile.wire_bytes()))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);
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
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(feed))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);

	let changelog = Changelog {
		protocol: 1,
		project_id: project_id.clone(),
		release_id: None,
		locale_sections: vec![LocaleSection {
			locale: "en".to_string(),
			sections: vec![ChangelogSection {
				heading: "Fixes".to_string(),
				body: "Removed the kraken crash on launch".to_string(),
				severity: None,
			}],
		}],
		declared_time: 1_760_000_000,
	};
	let signed = sign_payload(Kind::Changelog, &changelog, &[&signer]);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/changelog"))
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);

	let request = axum::http::Request::get("/v1/search?q=kraken")
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	let status = response.status();
	let bytes = axum::body::to_bytes(response.into_body(), 65536).await.expect("body");
	assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
	let page: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
	let results = page["results"].as_array().expect("results");
	assert_eq!(results.len(), 1);
	assert_eq!(results[0]["project_id"], project_id);
}

#[tokio::test]
async fn falls_back_to_a_labeled_near_match_for_a_typo() {
	let (application, _directory) = app().await;
	let signer = key(33);
	let (project_id, _release) = publish_project(&application, &signer).await;
	publish_profile(&application, &signer, &project_id, "Fabric Addon").await;

	let typo = axum::http::Request::get("/v1/search?q=fabrik")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(typo).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let page = body_json(response).await;
	let results = page["results"].as_array().expect("results");
	assert_eq!(results.len(), 1);
	assert_eq!(results[0]["project_id"], project_id);
	let annotations = results[0]["annotations"].as_array().expect("annotations");
	assert!(
		annotations.iter().any(|entry| entry["kind"] == "approximate-match"),
		"a near match must be labeled: {annotations:?}"
	);

	let genuine = axum::http::Request::get("/v1/search?q=fabric")
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(genuine).await.expect("response");
	let page = body_json(response).await;
	let results = page["results"].as_array().expect("results");
	assert_eq!(results.len(), 1);
	let annotations = results[0]["annotations"].as_array().expect("annotations");
	assert!(
		annotations.iter().all(|entry| entry["kind"] != "approximate-match"),
		"an exact match is not labeled approximate"
	);
}

#[tokio::test]
async fn filters_search_by_game_version_channel_and_state() {
	let (application, _directory) = app().await;
	let signer = key(35);
	let (project_id, _release) = publish_project(&application, &signer).await;
	publish_profile(&application, &signer, &project_id, "Fabric Addon").await;

	let search = |application: axum::Router, query: &str| {
		let uri = format!("/v1/search?{query}");
		async move {
			let request = axum::http::Request::get(uri).body(Body::empty()).expect("request");
			let response = application.oneshot(request).await.expect("response");
			body_json(response).await
		}
	};

	let page = search(application.clone(), "game_version=1.20.1").await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);
	let page = search(application.clone(), "game_version=1.19.0").await;
	assert!(page["results"].as_array().expect("results").is_empty());

	let page = search(application.clone(), "channel=release").await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);
	let page = search(application.clone(), "channel=beta").await;
	assert!(page["results"].as_array().expect("results").is_empty());

	let page = search(application.clone(), "state=listed").await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);

	let bad = axum::http::Request::get("/v1/search?state=sideways")
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(bad).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn filters_search_by_declared_loader_and_runtime_versions() {
	use moraine_model::compatibility::{Predicate, Scheme};

	let (application, _directory) = app().await;
	let signer = key(36);
	let (project_id, _release) = publish_project(&application, &signer).await;
	publish_profile(&application, &signer, &project_id, "Loader Addon").await;

	let (release, _) =
		release_wire_for_game_with_loader(&signer, &project_id, 0x51, "1.1.0", "1.20.1", Some("fabric"), Some("0.15.0"));
	store_release(&application, &project_id, release).await;
	let (runtime_release, _) = release_wire_with_runtime(
		&signer,
		&project_id,
		0x52,
		"1.2.0",
		Some(Predicate::new(Scheme::Exact, vec!["21".to_string()])),
	);
	store_release(&application, &project_id, runtime_release).await;

	let search = |application: axum::Router, query: &str| {
		let uri = format!("/v1/search?{query}");
		async move {
			let request = axum::http::Request::get(uri).body(Body::empty()).expect("request");
			let response = application.oneshot(request).await.expect("response");
			body_json(response).await
		}
	};
	let loader = sample_id("fabric");

	let page = search(application.clone(), &format!("loader={loader}&loader_version=0.15.0")).await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);
	let page = search(application.clone(), &format!("loader={loader}&loader_version=9.9.9")).await;
	assert!(page["results"].as_array().expect("results").is_empty());

	let page = search(application.clone(), "runtime_version=21").await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);
	let page = search(application.clone(), "runtime_version=17").await;
	assert!(page["results"].as_array().expect("results").is_empty());

	let bad = axum::http::Request::get("/v1/search?loader_version=0.15.0")
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(bad).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn counts_facets_with_every_other_filter_applied() {
	let (application, _directory) = app().await;
	let signer = key(37);
	let (project_id, _release) = publish_project(&application, &signer).await;
	publish_profile(&application, &signer, &project_id, "Fabric Physics Addon").await;

	let other = key(38);
	let (other_project, _release) = publish_project(&application, &other).await;
	publish_profile_for_game(&application, &other, &other_project, "Racing Aero Addon", "quake").await;
	let (release, _) = release_wire_for_game_id(&other, &other_project, 0x61, "1.1.0", "1.20.1", "quake", Some("rtx"), None);
	store_release(&application, &other_project, release).await;

	let facets = |application: axum::Router, query: &str| {
		let uri = format!("/v1/search/facets?{query}");
		async move {
			let request = axum::http::Request::get(uri).body(Body::empty()).expect("request");
			let response = application.oneshot(request).await.expect("response");
			assert_eq!(response.status(), StatusCode::OK);
			body_json(response).await
		}
	};
	let count = |page: &serde_json::Value, dimension: &str, value: &str| {
		page["facets"][dimension]
			.as_array()
			.expect("facet list")
			.iter()
			.find(|entry| entry["value"] == value)
			.map(|entry| entry["count"].as_i64().unwrap_or(0))
	};

	let page = facets(application.clone(), "").await;
	assert_eq!(count(&page, "game", &sample_id("minecraft")), Some(1));
	assert_eq!(count(&page, "game", &sample_id("quake")), Some(1));
	assert_eq!(count(&page, "loader", &sample_id("fabric")), Some(2));
	assert_eq!(count(&page, "loader", &sample_id("rtx")), Some(1));

	let page = facets(application.clone(), &format!("game={}", sample_id("minecraft"))).await;
	assert_eq!(
		count(&page, "game", &sample_id("quake")),
		Some(1),
		"the game facet ignores its own filter so alternatives stay visible"
	);
	assert_eq!(count(&page, "loader", &sample_id("rtx")), None);
	assert_eq!(count(&page, "loader", &sample_id("fabric")), Some(1));
}
