use axum::body::Body;
use axum::http::{StatusCode, header};
use tokio::net::TcpListener;
use tower::ServiceExt;

use crate::test_support::*;

#[tokio::test]
async fn a_sync_records_the_head_it_witnessed() {
	let home_signer = key(15);
	let (home, _home_directory) = app().await;
	let (project_id, release_digest) = publish_project(&home, &home_signer).await;
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
	let home_url = format!("http://127.0.0.1:{}", address.port());
	let sync = axum::http::Request::post("/v1/federation/sync")
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::COOKIE, format!("moraine_session={session}; moraine_csrf={csrf}"))
		.header("x-csrf-token", csrf.clone())
		.body(Body::from(
			serde_json::json!({ "home_url": home_url, "project_id": project_id }).to_string(),
		))
		.expect("request");
	let sync_response = directory.clone().oneshot(sync).await.expect("response");
	if sync_response.status() != StatusCode::OK {
		panic!(
			"sync failed: {} {}",
			sync_response.status(),
			String::from_utf8_lossy(&axum::body::to_bytes(sync_response.into_body(), 4096).await.expect("body"))
		);
	}

	let witness = axum::http::Request::get(format!("/v1/projects/{project_id}/witness"))
		.header(header::COOKIE, format!("moraine_session={session}"))
		.body(Body::empty())
		.expect("request");
	let response = directory.clone().oneshot(witness).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let log = body_json(response).await;
	let observations = log["observations"].as_array().expect("observations");
	assert_eq!(observations.len(), 1, "{log}");
	assert_eq!(observations[0]["source_home"], home_url);
	assert_eq!(observations[0]["sequence"], 1);
	assert!(log["conflicts"].as_array().expect("conflicts").is_empty());

	let head_entry = observations[0]["head_entry"].as_str().expect("head entry").to_string();
	let elsewhere = crate::registry::witness::WitnessObservationRow {
		source_home: "https://elsewhere.example".to_string(),
		sequence: 1,
		head_entry,
		observed_at: 1_760_000_100,
	};
	assert!(crate::registry::witness::witness_conflicts(std::slice::from_ref(&elsewhere)).is_empty());

	let rewritten = crate::registry::witness::WitnessObservationRow {
		source_home: elsewhere.source_home.clone(),
		sequence: 1,
		head_entry: format!("gd:sha256:{}", "aa".repeat(32)),
		observed_at: elsewhere.observed_at,
	};
	let split = crate::registry::witness::witness_conflicts(&[elsewhere, rewritten]);
	assert_eq!(split.len(), 1);
	assert_eq!(split[0].entries.len(), 2);
	assert_eq!(split[0].homes, vec!["https://elsewhere.example".to_string()]);
}
