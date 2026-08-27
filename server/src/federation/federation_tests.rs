use axum::body::Body;
use axum::http::{StatusCode, header};
use moraine_crypto::{ObjectKind as Kind, object_id};
use moraine_model::signed::sign_payload;
use tokio::net::TcpListener;
use tower::ServiceExt;

use crate::test_support::*;

#[tokio::test]
async fn federation_syncs_a_home_feed() {
	let home_signer = key(6);
	let (home, _home_directory) = app().await;
	let (project_id, release_digest) = publish_project(&home, &home_signer).await;
	let feed = feed_wire(&home_signer, &project_id, 1, None, release_digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(feed))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	let serving = home.clone();
	tokio::spawn(async move {
		let _ = axum::serve(listener, serving).await;
	});

	let (directory, _directory_dir) = app_mode(crate::config::Publishing::Review, true).await;
	let (session, csrf) = login(&directory, "ops@example.org").await;
	let sync = axum::http::Request::post("/v1/federation/sync")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::from(
			serde_json::json!({
				"home_url": format!("http://127.0.0.1:{}", address.port()),
				"project_id": project_id,
			})
			.to_string(),
		))
		.expect("request");
	let response = directory.clone().oneshot(sync).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let report = body_json(response).await;
	assert_eq!(report["applied"], 1);

	let feed_request = axum::http::Request::get(format!("/v1/projects/{project_id}/feed"))
		.body(Body::empty())
		.expect("request");
	let response = directory.clone().oneshot(feed_request).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["head_seq"], 1);

	let subscriptions = axum::http::Request::get("/v1/subscriptions")
		.header(header::COOKIE, format!("moraine_session={session}"))
		.body(Body::empty())
		.expect("request");
	let response = directory.clone().oneshot(subscriptions).await.expect("response");
	let list = body_json(response).await;
	assert_eq!(list.as_array().expect("subscriptions").len(), 1);
	assert_eq!(list[0]["cursor_seq"], 1);
	assert_eq!(list[0]["lag_entries"], 0);

	let unsubscribe = axum::http::Request::builder()
		.method("DELETE")
		.uri(format!(
			"/v1/subscriptions?home_url={}&project_id={project_id}",
			urlencoding_home(&address)
		))
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::empty())
		.expect("request");
	let response = directory.clone().oneshot(unsubscribe).await.expect("response");
	assert_eq!(response.status(), StatusCode::NO_CONTENT);

	let subscriptions = axum::http::Request::get("/v1/subscriptions")
		.header(header::COOKIE, format!("moraine_session={session}"))
		.body(Body::empty())
		.expect("request");
	let response = directory.oneshot(subscriptions).await.expect("response");
	let list = body_json(response).await;
	assert!(list.as_array().expect("subscriptions").is_empty());
}

fn urlencoding_home(address: &std::net::SocketAddr) -> String {
	format!("http://127.0.0.1:{}", address.port())
		.replace(':', "%3A")
		.replace('/', "%2F")
}

#[tokio::test]
async fn federation_follows_a_changelog_referenced_by_a_release() {
	use moraine_model::changelog::{Changelog, ChangelogSection, LocaleSection};

	let home_signer = key(22);
	let (home, _home_directory) = app_with_limit(crate::config::Publishing::Open, false, 1).await;
	let project = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis_wire(
			&home_signer,
			&["delegation", "release", "profile", "changelog"],
		)))
		.expect("request");
	let response = home.clone().oneshot(project).await.expect("response");
	let project_id = body_json(response).await["project_id"]
		.as_str()
		.expect("project id")
		.to_string();

	let changelog = Changelog {
		protocol: 1,
		project_id: project_id.clone(),
		release_id: None,
		locale_sections: vec![LocaleSection {
			locale: "en".to_string(),
			sections: vec![ChangelogSection {
				heading: "Fixes".to_string(),
				body: "Removed the kraken crash".to_string(),
				severity: None,
			}],
		}],
		declared_time: 1_760_000_000,
	};
	let signed_changelog = sign_payload(Kind::Changelog, &changelog, &[&home_signer]);
	let changelog_digest = object_id(Kind::Changelog, &signed_changelog.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/changelog"))
		.body(Body::from(signed_changelog.wire_bytes()))
		.expect("request");
	assert_eq!(
		home.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);

	let (release, release_digest) = release_wire_with_changelog(&home_signer, &project_id, 0x52, "1.0.0", changelog_digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(release))
		.expect("request");
	assert_eq!(
		home.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);
	let feed = feed_wire(&home_signer, &project_id, 1, None, release_digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(feed))
		.expect("request");
	assert_eq!(
		home.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);

	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	let serving = home.clone();
	tokio::spawn(async move {
		let _ = axum::serve(listener, serving).await;
	});

	let (directory, _directory_dir) = app_mode(crate::config::Publishing::Review, true).await;
	let (session, csrf) = login(&directory, "ops@example.org").await;
	let sync = axum::http::Request::post("/v1/federation/sync")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::from(
			serde_json::json!({
				"home_url": format!("http://127.0.0.1:{}", address.port()),
				"project_id": project_id,
			})
			.to_string(),
		))
		.expect("request");
	let response = directory.clone().oneshot(sync).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let changelog_request = axum::http::Request::get(format!(
		"/v1/projects/{project_id}/changelog/{}",
		hex::encode(changelog_digest)
	))
	.body(Body::empty())
	.expect("request");
	let response = directory.oneshot(changelog_request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["locale_sections"][0]["sections"][0]["heading"], "Fixes");
}

#[tokio::test]
async fn detects_a_home_that_equivocates_on_the_same_sequence() {
	let signer = key(32);
	let genesis = genesis_wire(&signer, PROJECT_KINDS);

	async fn serve(
		signer: &moraine_crypto::SigningKey,
		genesis: &[u8],
		release_nonce: u8,
		version: &str,
	) -> (axum::Router, tempfile::TempDir, String) {
		let (home, directory) = app_with_limit(crate::config::Publishing::Open, false, 1).await;
		let request = axum::http::Request::post("/v1/projects")
			.body(Body::from(genesis.to_vec()))
			.expect("request");
		let response = home.clone().oneshot(request).await.expect("response");
		let project_id = body_json(response).await["project_id"]
			.as_str()
			.expect("project id")
			.to_string();
		let (release, digest) = release_wire_variant(signer, &project_id, release_nonce, version);
		let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
			.body(Body::from(release))
			.expect("request");
		assert_eq!(
			home.clone().oneshot(request).await.expect("response").status(),
			StatusCode::CREATED
		);
		let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
			.body(Body::from(feed_wire(signer, &project_id, 1, None, digest)))
			.expect("request");
		assert_eq!(
			home.clone().oneshot(request).await.expect("response").status(),
			StatusCode::CREATED
		);
		(home, directory, project_id)
	}

	let (home_a, _dir_a, project_id) = serve(&signer, &genesis, 0xA1, "1.0.0").await;
	let (home_b, _dir_b, _same_id) = serve(&signer, &genesis, 0xB2, "2.0.0").await;

	let (directory, _directory_dir) = app_mode(crate::config::Publishing::Review, true).await;
	let (session, csrf) = login(&directory, "ops@example.org").await;

	let sync = |home: axum::Router| {
		let session = session.clone();
		let csrf = csrf.clone();
		let project_id = project_id.clone();
		async move {
			let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
			let address = listener.local_addr().expect("addr");
			tokio::spawn(async move {
				let _ = axum::serve(listener, home).await;
			});
			axum::http::Request::post("/v1/federation/sync")
				.header(header::CONTENT_TYPE, "application/json")
				.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
				.header("x-csrf-token", csrf)
				.body(Body::from(
					serde_json::json!({
						"home_url": format!("http://127.0.0.1:{}", address.port()),
						"project_id": project_id,
					})
					.to_string(),
				))
				.expect("request")
		}
	};

	let request = sync(home_a).await;
	let response = directory.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let request = sync(home_b).await;
	let response = directory.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
	let body = axum::body::to_bytes(response.into_body(), 4096).await.expect("body");
	assert!(
		String::from_utf8_lossy(&body).contains("equivocated"),
		"{}",
		String::from_utf8_lossy(&body)
	);
}

#[tokio::test]
async fn federation_syncs_a_game_definition() {
	let (home, _home_directory) = app().await;
	let key = key(16);
	let request = axum::http::Request::post("/v1/games")
		.body(Body::from(game_genesis_wire(&key)))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let game_id = body_json(response).await["id"].as_str().expect("game id").to_string();

	let request = axum::http::Request::post(format!("/v1/games/{game_id}/definitions"))
		.body(Body::from(game_definition_wire(&key, &game_id)))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	let serving = home.clone();
	tokio::spawn(async move {
		let _ = axum::serve(listener, serving).await;
	});

	let (directory, _directory_dir) = app_mode(crate::config::Publishing::Open, true).await;
	let (session, csrf) = login(&directory, "ops@example.org").await;
	let sync = axum::http::Request::post("/v1/federation/sync-definition")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(
			serde_json::json!({
				"home_url": format!("http://127.0.0.1:{}", address.port()),
				"id": game_id,
				"kind": "game",
			})
			.to_string(),
		))
		.expect("request");
	let response = directory.clone().oneshot(sync).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let get = axum::http::Request::get(format!("/v1/games/{game_id}"))
		.body(Body::empty())
		.expect("request");
	let response = directory.oneshot(get).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let view = body_json(response).await;
	assert_eq!(view["payload"]["display_name"], "Minecraft");
	assert_eq!(view["payload"]["version_ordering"], "semver");
}

#[tokio::test]
async fn federation_syncs_a_loader_with_its_releases() {
	let (home, _home_directory) = app().await;
	let key = key(23);
	let request = axum::http::Request::post("/v1/loaders")
		.body(Body::from(loader_genesis_wire(&key)))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let loader_id = body_json(response).await["id"].as_str().expect("id").to_string();

	let game_id = sample_id("minecraft");
	let request = axum::http::Request::post(format!("/v1/loaders/{loader_id}/definitions"))
		.body(Body::from(loader_definition_wire(&key, &loader_id, &game_id)))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let request = axum::http::Request::post(format!("/v1/loaders/{loader_id}/definitions"))
		.body(Body::from(loader_release_wire(&key, &loader_id, "0.15.0")))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	let serving = home.clone();
	tokio::spawn(async move {
		let _ = axum::serve(listener, serving).await;
	});

	let (directory, _directory_dir) = app_mode(crate::config::Publishing::Open, true).await;
	let (session, csrf) = login(&directory, "ops@example.org").await;
	let sync = axum::http::Request::post("/v1/federation/sync-definition")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(
			serde_json::json!({
				"home_url": format!("http://127.0.0.1:{}", address.port()),
				"id": loader_id,
				"kind": "loader",
			})
			.to_string(),
		))
		.expect("request");
	let response = directory.clone().oneshot(sync).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let list = axum::http::Request::get(format!("/v1/loaders/{loader_id}/releases"))
		.body(Body::empty())
		.expect("request");
	let response = directory.oneshot(list).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let releases = body_json(response).await;
	assert_eq!(releases.as_array().expect("releases").len(), 1);
	assert_eq!(releases[0]["version"], "0.15.0");
}

#[tokio::test]
async fn subscribes_to_and_lists_a_definition() {
	let (home, _home_directory) = app().await;
	let signer = key(19);
	let request = axum::http::Request::post("/v1/games")
		.body(Body::from(game_genesis_wire(&signer)))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let game_id = body_json(response).await["id"].as_str().expect("game id").to_string();

	let request = axum::http::Request::post(format!("/v1/games/{game_id}/definitions"))
		.body(Body::from(game_definition_wire(&signer, &game_id)))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	let serving = home.clone();
	tokio::spawn(async move {
		let _ = axum::serve(listener, serving).await;
	});

	let (directory, _directory_dir) = app_mode(crate::config::Publishing::Open, true).await;
	let (session, csrf) = login(&directory, "ops@example.org").await;
	let subscribe = axum::http::Request::post("/v1/federation/subscribe-definition")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::from(
			serde_json::json!({
				"home_url": format!("http://127.0.0.1:{}", address.port()),
				"id": game_id,
				"kind": "game",
			})
			.to_string(),
		))
		.expect("request");
	let response = directory.clone().oneshot(subscribe).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let listing = axum::http::Request::get("/v1/definition-subscriptions")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.body(Body::empty())
		.expect("request");
	let response = directory.clone().oneshot(listing).await.expect("response");
	let list = body_json(response).await;
	assert_eq!(list.as_array().expect("subscriptions").len(), 1);
	assert_eq!(list[0]["id"], game_id);

	let get = axum::http::Request::get(format!("/v1/games/{game_id}"))
		.body(Body::empty())
		.expect("request");
	let response = directory.oneshot(get).await.expect("response");
	let view = body_json(response).await;
	assert_eq!(view["payload"]["display_name"], "Minecraft");
}

#[tokio::test]
async fn bounds_the_home_response_body() {
	let mock = axum::Router::new()
		.route("/small", axum::routing::get(|| async { b"ok".to_vec() }))
		.route("/huge", axum::routing::get(|| async { vec![b'x'; 4096] }));
	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	tokio::spawn(async move {
		let _ = axum::serve(listener, mock).await;
	});
	let base = format!("http://127.0.0.1:{}", address.port());
	let client = super::HomeClient::new(&base, true, 1024, &[]).await.expect("client");
	assert_eq!(client.get_bytes("small").await.expect("small"), b"ok");
	assert!(client.get_bytes("huge").await.is_err());
}

#[tokio::test]
async fn reaches_a_local_home_by_name_not_only_by_literal() {
	let mock = axum::Router::new().route("/small", axum::routing::get(|| async { b"ok".to_vec() }));
	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	tokio::spawn(async move {
		let _ = axum::serve(listener, mock).await;
	});
	let base = format!("http://localhost:{}", address.port());
	let client = super::HomeClient::new(&base, true, 1024, &[]).await.expect("client");
	assert_eq!(client.get_bytes("small").await.expect("small"), b"ok");
}

#[tokio::test]
async fn federation_paginates_through_a_multi_page_feed() {
	let home_signer = key(7);
	let (home, _home_directory) = app_with_limit(crate::config::Publishing::Open, false, 1).await;
	let (project_id, first_release) = publish_project(&home, &home_signer).await;

	let mut previous: Option<[u8; 32]> = None;
	let mut object_digest = first_release;
	for index in 0..3u8 {
		if index > 0 {
			let (release, digest) = release_wire_variant(&home_signer, &project_id, 0x50 + index, &format!("1.0.{index}"));
			let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
				.body(Body::from(release))
				.expect("request");
			let response = home.clone().oneshot(request).await.expect("response");
			assert_eq!(response.status(), StatusCode::CREATED);
			object_digest = digest;
		}
		let feed = feed_wire(&home_signer, &project_id, index as u64 + 1, previous, object_digest);
		let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
			.body(Body::from(feed))
			.expect("request");
		let response = home.clone().oneshot(request).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);
		let receipt = body_json(response).await;
		previous = Some(id_bytes(receipt["entry"].as_str().expect("entry id")));
	}

	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	let serving = home.clone();
	tokio::spawn(async move {
		let _ = axum::serve(listener, serving).await;
	});

	let (directory, _directory_dir) = app_mode(crate::config::Publishing::Review, true).await;
	let (session, csrf) = login(&directory, "ops@example.org").await;
	let sync = axum::http::Request::post("/v1/federation/sync")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(
			serde_json::json!({
				"home_url": format!("http://127.0.0.1:{}", address.port()),
				"project_id": project_id,
			})
			.to_string(),
		))
		.expect("request");
	let response = directory.clone().oneshot(sync).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let report = body_json(response).await;
	assert_eq!(report["applied"], 3);
	assert_eq!(report["head_seq"], 3);
}

#[tokio::test]
async fn resync_pulls_entries_published_after_the_first_sync() {
	let home_signer = key(8);
	let (home, _home_directory) = app().await;
	let (project_id, first_release) = publish_project(&home, &home_signer).await;
	let first = feed_wire(&home_signer, &project_id, 1, None, first_release);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(first))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	let entry_one = id_bytes(body_json(response).await["entry"].as_str().expect("entry id"));

	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	let serving = home.clone();
	tokio::spawn(async move {
		let _ = axum::serve(listener, serving).await;
	});

	let (directory, _directory_dir) = app_mode(crate::config::Publishing::Review, true).await;
	let (session, csrf) = login(&directory, "ops@example.org").await;
	let cookie = format!("moraine_session={session}; moraine_csrf={csrf}");
	let home_url = format!("http://127.0.0.1:{}", address.port());
	let sync = axum::http::Request::post("/v1/federation/sync")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, cookie.clone())
		.header("x-csrf-token", csrf.clone())
		.body(Body::from(
			serde_json::json!({ "home_url": home_url, "project_id": project_id }).to_string(),
		))
		.expect("request");
	let response = directory.clone().oneshot(sync).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);

	let (second_release, second_digest) = release_wire_variant(&home_signer, &project_id, 0x61, "2.0.0");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
		.body(Body::from(second_release))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let second = feed_wire(&home_signer, &project_id, 2, Some(entry_one), second_digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(second))
		.expect("request");
	let response = home.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let resync = axum::http::Request::post("/v1/federation/resync")
		.header(header::COOKIE, cookie.clone())
		.header("x-csrf-token", csrf)
		.body(Body::empty())
		.expect("request");
	let response = directory.clone().oneshot(resync).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let report = body_json(response).await;
	assert_eq!(report["synced"], 1);
	assert_eq!(report["failed"], 0);

	let feed = axum::http::Request::get(format!("/v1/projects/{project_id}/feed"))
		.body(Body::empty())
		.expect("request");
	let response = directory.oneshot(feed).await.expect("response");
	let page = body_json(response).await;
	assert_eq!(page["head_seq"], 2);
}

#[tokio::test]
async fn rejects_a_home_whose_feed_went_backwards() {
	use std::sync::Arc;
	use std::sync::atomic::{AtomicI64, Ordering};

	use axum::extract::State;

	let signer = key(12);
	let wire = genesis_wire(&signer, PROJECT_KINDS);
	let (_, object) = crate::verify::verify_genesis(&wire).expect("genesis");
	let project_id = object.id;
	let genesis_id = project_id.clone();
	let head = Arc::new(AtomicI64::new(1));

	let mock = axum::Router::new()
		.route(
			"/v1/projects/{id}",
			axum::routing::get(|axum::extract::Path(id): axum::extract::Path<String>| async move {
				axum::Json(serde_json::json!({ "project_id": id, "genesis": id }))
			}),
		)
		.route(
			"/v1/objects/{hex}",
			axum::routing::get({
				let wire = wire.clone();
				move || {
					let wire = wire.clone();
					async move { wire }
				}
			}),
		)
		.route(
			"/v1/projects/{id}/feed",
			axum::routing::get({
				let head = head.clone();
				move |State(_): State<()>| {
					let head = head.clone();
					async move {
						axum::Json(serde_json::json!({
							"project_id": genesis_id,
							"head_seq": head.load(Ordering::Relaxed),
							"entries": [],
						}))
					}
				}
			}),
		)
		.with_state(());

	let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let address = listener.local_addr().expect("addr");
	tokio::spawn(async move {
		let _ = axum::serve(listener, mock).await;
	});

	let (directory, directory_dir) = app_mode(crate::config::Publishing::Review, true).await;
	let home_url = format!("http://127.0.0.1:{}", address.port());
	let metadata = crate::test_support::store_for(directory_dir.path()).await;
	metadata
		.upsert_subscription(&home_url, &project_id, "active", 1)
		.await
		.expect("subscribe");
	metadata
		.set_subscription_cursor(&home_url, &project_id, 5, 5, "active", 2)
		.await
		.expect("cursor");

	let (session, csrf) = login(&directory, "ops@example.org").await;
	let sync = axum::http::Request::post("/v1/federation/sync")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(
			serde_json::json!({ "home_url": home_url, "project_id": project_id }).to_string(),
		))
		.expect("request");
	let response = directory.clone().oneshot(sync).await.expect("response");
	assert_eq!(response.status(), StatusCode::CONFLICT);

	let subscription = metadata
		.subscription(&home_url, &project_id)
		.await
		.expect("subscription")
		.expect("present");
	assert_eq!(subscription.cursor_seq, 5);

	let (session, csrf) = login(&directory, "ops@example.org").await;
	let reset = axum::http::Request::builder()
		.method("POST")
		.uri(format!(
			"/v1/subscriptions/reset?home_url={}&project_id={project_id}&cursor=1",
			urlencoding_home(&address)
		))
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::empty())
		.expect("request");
	let response = directory.clone().oneshot(reset).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let reset_row = metadata
		.subscription(&home_url, &project_id)
		.await
		.expect("subscription")
		.expect("present");
	assert_eq!(reset_row.reset_count, 1);

	let sync = axum::http::Request::post("/v1/federation/sync")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf)
		.body(Body::from(
			serde_json::json!({ "home_url": home_url, "project_id": project_id }).to_string(),
		))
		.expect("request");
	let response = directory.oneshot(sync).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
}
