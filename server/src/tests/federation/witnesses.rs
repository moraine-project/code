use axum::body::Body;
use axum::http::{StatusCode, header};
use moraine_crypto::SigningKey;
use moraine_model::Canonical;
use moraine_model::witness::{WITNESS_DOMAIN, WitnessBundle, WitnessObservation};
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
		observer_id: "local".to_string(),
		source_home: "https://elsewhere.example".to_string(),
		sequence: 1,
		head_entry,
		observed_at: 1_760_000_100,
	};
	assert!(crate::registry::witness::witness_conflicts(std::slice::from_ref(&elsewhere)).is_empty());

	let rewritten = crate::registry::witness::WitnessObservationRow {
		observer_id: "local".to_string(),
		source_home: "https://elsewhere.example".to_string(),
		sequence: 1,
		head_entry: format!("gd:sha256:{}", "aa".repeat(32)),
		observed_at: 1_760_000_100,
	};
	let split = crate::registry::witness::witness_conflicts(&[elsewhere, rewritten]);
	assert_eq!(split.len(), 1);
	assert_eq!(split[0].entries.len(), 2);
	assert_eq!(split[0].source_home, "https://elsewhere.example");
	assert_eq!(split[0].sequence, 1);
}

#[test]
fn a_different_home_at_the_same_sequence_is_not_a_conflict() {
	use crate::registry::witness::{WitnessObservationRow, witness_conflicts};

	let row = |home: &str, entry: &str, observer: &str| WitnessObservationRow {
		observer_id: observer.to_string(),
		source_home: home.to_string(),
		sequence: 7,
		head_entry: entry.to_string(),
		observed_at: 1_760_000_000,
	};
	let left = row("https://a.example", "gd:sha256:aa", "local");
	let right = row("https://b.example", "gd:sha256:bb", "local");
	assert!(
		witness_conflicts(&[left, right]).is_empty(),
		"two homes at one sequence are independent, not equivocation"
	);

	let third_party = row("https://a.example", "gd:sha256:bb", "ed25519:thirdparty");
	let corroborated = witness_conflicts(&[row("https://a.example", "gd:sha256:aa", "local"), third_party]);
	assert_eq!(corroborated.len(), 1, "observers disagreeing about one home is a fork");
	assert_eq!(corroborated[0].source_home, "https://a.example");
	assert_eq!(corroborated[0].observers, vec!["local", "ed25519:thirdparty"]);

	let agreed = witness_conflicts(&[
		row("https://a.example", "gd:sha256:aa", "local"),
		row("https://a.example", "gd:sha256:aa", "ed25519:thirdparty"),
	]);
	assert!(agreed.is_empty(), "observers agreeing is not a conflict");
}

#[tokio::test]
async fn imports_signed_witness_observations_and_exposes_the_observer() {
	let (application, _directory) = app().await;
	let signer = SigningKey::from_seed(&[77u8; 32]);
	let bundle = WitnessBundle {
		protocol: 1,
		observer_id: signer.key_id().to_string(),
		observations: vec![WitnessObservation {
			project_id: "gd:sha256:11".to_string(),
			source_home: "https://home.example".to_string(),
			sequence: 4,
			head_entry: "gd:sha256:22".to_string(),
			observed_at: 1_760_000_400,
		}],
	};
	let payload = bundle.to_canonical_bytes();
	let mut message = WITNESS_DOMAIN.to_vec();
	message.extend_from_slice(&payload);
	let mut invalid_signature = signer.sign(&message);
	invalid_signature[0] ^= 1;
	let invalid_import = axum::http::Request::post("/v1/federation/witness")
		.header(header::CONTENT_TYPE, "application/json")
		.body(Body::from(
			serde_json::json!({
				"payload": hex::encode(&payload),
				"signature": hex::encode(invalid_signature),
				"public_key": hex::encode(signer.verifying_key().to_bytes()),
			})
			.to_string(),
		))
		.expect("request");
	let response = application.clone().oneshot(invalid_import).await.expect("response");
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
	let import = axum::http::Request::post("/v1/federation/witness")
		.header(header::CONTENT_TYPE, "application/json")
		.body(Body::from(
			serde_json::json!({
				"payload": hex::encode(&payload),
				"signature": hex::encode(signer.sign(&message)),
				"public_key": hex::encode(signer.verifying_key().to_bytes()),
			})
			.to_string(),
		))
		.expect("request");
	let response = application.clone().oneshot(import).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let (session, _) = login(&application, "ops@example.org").await;
	let observe = axum::http::Request::get("/v1/projects/gd:sha256:11/witness")
		.header(header::COOKIE, format!("moraine_session={session}"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(observe).await.expect("response");
	let view = body_json(response).await;
	assert_eq!(view["observations"][0]["observer_id"], signer.key_id().to_string());
}
