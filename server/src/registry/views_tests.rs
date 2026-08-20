use axum::body::Body;
use axum::http::header;
use moraine_crypto::{ObjectKind as Kind, object_id};
use moraine_model::advisory::{Advisory, Affected, Category, Severity};
use moraine_model::compatibility::Side;
use moraine_model::dependency::TargetKind;
use moraine_model::modpack::{ModpackEntry, ModpackManifest, ModpackOverride};
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use super::*;
use crate::test_support::*;

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

	let described = axum::http::Request::get("/v1/search?q=longer")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(described).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);

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

	let by_created = axum::http::Request::get("/v1/search?sort=created")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(by_created).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let by_popularity = axum::http::Request::get("/v1/search?sort=popularity")
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(by_popularity).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let page = body_json(response).await;
	assert_eq!(page["results"].as_array().expect("results").len(), 1);

	let unsupported = axum::http::Request::get("/v1/search?sort=banana")
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
	let (project_id, release_digest) =
		publish_project_with_kinds(&application, &signer, &["delegation", "release", "profile", "modpack"]).await;

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
async fn serves_a_changelog_and_names_it_from_the_release() {
	use moraine_model::changelog::{Changelog, ChangelogSection, LocaleSection};
	use moraine_model::release::{ReleaseObject, ReleasePayload};

	let (application, _directory) = app().await;
	let signer = key(21);
	let (project_id, _release) =
		publish_project_with_kinds(&application, &signer, &["delegation", "release", "profile", "changelog"]).await;
	let changelog = Changelog {
		protocol: 1,
		project_id: project_id.clone(),
		release_id: None,
		locale_sections: vec![LocaleSection {
			locale: "en".to_string(),
			sections: vec![ChangelogSection {
				heading: "Fixes".to_string(),
				body: "Removed the kraken crash".to_string(),
				severity: Some("high".to_string()),
			}],
		}],
		declared_time: 1_760_000_000,
	};
	let signed_changelog = sign_payload(Kind::Changelog, &changelog, &[&signer]);
	let changelog_digest = object_id(Kind::Changelog, &signed_changelog.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/changelog"))
		.body(Body::from(signed_changelog.wire_bytes()))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);

	let release = ReleasePayload {
		protocol: 1,
		project_id: project_id.clone(),
		game_id: sample_id("minecraft"),
		release_nonce: vec![0x41; 16],
		human_version: "1.0.0".to_string(),
		channel: "release".to_string(),
		kind: "mod".to_string(),
		declared_time: 1_760_000_000,
		compatibility: vec![moraine_model::compatibility::Compatibility {
			game_version_predicate: moraine_model::compatibility::Predicate::new(
				moraine_model::compatibility::Scheme::Exact,
				vec!["1.20.1".to_string()],
			),
			loader_id: None,
			loader_version_predicate: None,
			side: Side::Both,
			runtime_predicate: None,
			os_predicate: None,
			arch_predicate: None,
		}],
		artifacts: vec![moraine_model::artifact::Artifact {
			digest: vec![0xCD; 32],
			size: 12,
			media_type: "application/java-archive".to_string(),
			filename: "example.jar".to_string(),
			is_primary: true,
			os_predicate: None,
			arch_predicate: None,
		}],
		dependencies: Vec::new(),
		source_reference: None,
		changelog_digest: Some(changelog_digest.to_vec()),
		license_expression: None,
		rights: None,
		sbom_digest: None,
		minimum_verifier_version: 1,
		critical_extensions: Vec::new(),
	};
	let signed_release = sign_payload(Kind::Release, &release, &[&signer]);
	let decoded = ReleaseObject::from_canonical_bytes(&signed_release.payload_bytes).expect("decode");
	let ReleaseObject::Release(decoded) = decoded else {
		panic!("expected a release payload");
	};
	assert_eq!(decoded.changelog_digest.as_deref(), Some(changelog_digest.as_slice()));
	let release_digest = object_id(Kind::Release, &signed_release.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(signed_release.wire_bytes()))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);

	let view_request =
		axum::http::Request::get(format!("/v1/projects/{project_id}/releases/{}", hex::encode(release_digest)))
			.body(Body::empty())
			.expect("request");
	let response = application.clone().oneshot(view_request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["changelog"], format!("gd:sha256:{}", hex::encode(changelog_digest)));

	let changelog_request = axum::http::Request::get(format!(
		"/v1/projects/{project_id}/changelog/{}",
		hex::encode(changelog_digest)
	))
	.body(Body::empty())
	.expect("request");
	let response = application.clone().oneshot(changelog_request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["locale_sections"][0]["locale"], "en");
	assert_eq!(view["locale_sections"][0]["sections"][0]["heading"], "Fixes");
	assert_eq!(view["locale_sections"][0]["sections"][0]["severity"], "high");
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
