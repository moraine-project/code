use axum::body::Body;
use axum::http::{StatusCode, header};
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
	let response = directory.oneshot(subscriptions).await.expect("response");
	let list = body_json(response).await;
	assert_eq!(list.as_array().expect("subscriptions").len(), 1);
	assert_eq!(list[0]["cursor_seq"], 1);
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
