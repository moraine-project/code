use axum::body::Body;
use axum::http::StatusCode;
use moraine_crypto::{ObjectKind as Kind, SigningKey, object_id};
use moraine_model::delegation::{Delegation, Migration};
use moraine_model::signed::sign_payload;
use tower::ServiceExt;

use crate::test_support::*;

fn migration(project_id: &str) -> Delegation {
	Delegation::Migration(Migration {
		protocol: 1,
		project_id: project_id.to_string(),
		old_home: "https://old.example".to_string(),
		new_home: "https://new.example".to_string(),
		cutover_seq: 5,
		reason: None,
		declared_time: 1_760_000_000,
	})
}

#[tokio::test]
async fn records_a_cross_signed_migration_and_refuses_an_under_signed_one() {
	let (application, _directory) = app().await;
	let publisher = SigningKey::from_seed(&[0xD1; 32]);
	let old_home = SigningKey::from_seed(&[0xD2; 32]);
	let new_home = SigningKey::from_seed(&[0xD3; 32]);

	let genesis = genesis_wire_roots(&[&publisher, &old_home, &new_home], PROJECT_KINDS);
	let request = axum::http::Request::post("/v1/projects")
		.body(Body::from(genesis))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);
	let project_id = body_json(response).await["project_id"]
		.as_str()
		.expect("project id")
		.to_string();

	let signed = sign_payload(Kind::Delegation, &migration(&project_id), &[&publisher, &old_home, &new_home])
		.expect("valid signed migration");
	let digest = object_id(Kind::Delegation, &signed.payload_bytes);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/delegation"))
		.body(Body::from(signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let under_signed =
		sign_payload(Kind::Delegation, &migration(&project_id), &[&publisher]).expect("valid signed migration");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/delegation"))
		.body(Body::from(under_signed.wire_bytes()))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(
		response.status(),
		StatusCode::BAD_REQUEST,
		"a migration without the home cross-signatures is refused"
	);

	let entry = feed_wire_kind(&publisher, &project_id, 1, None, digest, "migration");
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::CREATED);

	let list = axum::http::Request::get(format!("/v1/projects/{project_id}/migrations"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(list).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let migrations = body_json(response).await;
	assert_eq!(migrations.as_array().expect("migrations").len(), 1);
	assert_eq!(migrations[0]["new_home"], "https://new.example");
	assert_eq!(migrations[0]["cutover_seq"], 5);
}
