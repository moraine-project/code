use axum::body::Body;
use axum::http::StatusCode;
use moraine_crypto::SigningKey;
use tower::ServiceExt;

use crate::test_support::*;

#[tokio::test]
async fn lists_acceptance_mappings_and_refuses_a_stale_one() {
	let key = SigningKey::from_seed(&[0x83; 32]);
	let (application, _directory) = app().await;
	let request = axum::http::Request::builder()
		.method("POST")
		.uri("/v1/loaders")
		.header(axum::http::header::CONTENT_TYPE, "application/json")
		.body(Body::from(loader_genesis_wire(&key)))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let loader_id = body_json(response).await["id"].as_str().expect("id").to_string();

	let put = |body: Vec<u8>| {
		axum::http::Request::post(format!("/v1/loaders/{loader_id}/definitions"))
			.body(Body::from(body))
			.expect("request")
	};
	let response = application
		.clone()
		.oneshot(put(loader_definition_wire(&key, &loader_id, "gd:sha256:00")))
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let accepted = "gd:sha256:aa";
	let response = application
		.clone()
		.oneshot(put(loader_acceptance_wire(
			&key,
			&loader_id,
			accepted,
			"gd:sha256:00",
			1_760_000_000,
		)))
		.await
		.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let list = axum::http::Request::get(format!("/v1/loaders/{loader_id}/accepts"))
		.body(Body::empty())
		.expect("request");
	let response = application.clone().oneshot(list).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let accepts = body_json(response).await;
	assert_eq!(accepts.as_array().expect("accepts").len(), 1);
	assert_eq!(accepts[0]["accepted_loader_id"], accepted);
	assert_eq!(accepts[0]["qualification"], "native");
	assert_eq!(accepts[0]["declared_by"]["kind"], "loader-authority");

	let stale = application
		.clone()
		.oneshot(put(loader_acceptance_wire(
			&key,
			&loader_id,
			accepted,
			"gd:sha256:00",
			1_759_000_000,
		)))
		.await
		.expect("response");
	assert_eq!(stale.status(), StatusCode::CONFLICT);
}
