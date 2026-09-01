use axum::body::Body;
use axum::http::StatusCode;
use tower::ServiceExt;

use crate::test_support::*;

#[tokio::test]
async fn reports_the_latest_release_per_channel() {
	let (application, _directory) = app().await;
	let signer = key(43);
	let (project_id, first) = publish_project(&application, &signer).await;

	let mut previous = None;
	for (index, (nonce, version, channel)) in [
		(0x11u8, "1.0.0", "release"),
		(0x12, "2.0.0", "release"),
		(0x13, "1.5.0-beta", "beta"),
	]
	.into_iter()
	.enumerate()
	{
		let seq = index as u64 + 1;
		let (release, digest) = release_wire_with_channel(&signer, &project_id, nonce, version, channel);
		let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
			.body(Body::from(release))
			.expect("request");
		assert_eq!(
			application.clone().oneshot(request).await.expect("response").status(),
			StatusCode::CREATED
		);
		let entry = feed_wire_kind(&signer, &project_id, seq, previous, digest, "release-published");
		let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
			.body(Body::from(entry))
			.expect("request");
		let response = application.clone().oneshot(request).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);
		previous = Some(id_bytes(body_json(response).await["entry"].as_str().expect("entry")));
	}
	let _ = first;

	let request = axum::http::Request::get(format!("/v1/projects/{project_id}/channels"))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	let channels = body_json(response).await;
	let channels = channels.as_array().expect("channels");
	assert_eq!(channels.len(), 2);
	assert_eq!(channels[0]["channel"], "beta");
	assert_eq!(channels[0]["human_version"], "1.5.0-beta");
	assert_eq!(channels[1]["channel"], "release");
	assert_eq!(channels[1]["human_version"], "2.0.0");
	assert_eq!(channels[1]["seq"], 2);
}
