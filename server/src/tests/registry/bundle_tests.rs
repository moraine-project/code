use std::io::Read;

use axum::body::Body;
use axum::http::{StatusCode, header};
use tower::ServiceExt;

use crate::test_support::*;

#[tokio::test]
async fn serves_a_verification_bundle_for_a_release() {
	let (application, _directory) = app().await;
	let signer = key(34);
	let (project_id, release_digest) = publish_project(&application, &signer).await;
	let entry = feed_wire(&signer, &project_id, 1, None, release_digest);
	let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
		.body(Body::from(entry))
		.expect("request");
	assert_eq!(
		application.clone().oneshot(request).await.expect("response").status(),
		StatusCode::CREATED
	);

	let request = axum::http::Request::get(format!(
		"/v1/projects/{project_id}/releases/{}/bundle",
		hex::encode(release_digest)
	))
	.body(Body::empty())
	.expect("request");
	let response = application.clone().oneshot(request).await.expect("response");
	assert_eq!(response.status(), StatusCode::OK);
	assert_eq!(
		response
			.headers()
			.get(header::CONTENT_TYPE)
			.and_then(|value| value.to_str().ok()),
		Some("application/zip")
	);
	assert!(
		response
			.headers()
			.get(header::CONTENT_DISPOSITION)
			.and_then(|value| value.to_str().ok())
			.is_some_and(|value| value.starts_with("attachment")),
		"the bundle is offered as a download"
	);
	let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
		.await
		.expect("body");

	let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
	let mut names: Vec<String> = (0..archive.len())
		.filter_map(|index| Some(archive.by_index(index).ok()?.name().to_string()))
		.collect();
	names.sort();
	assert!(names.contains(&"genesis.cbor".to_string()), "{names:?}");
	assert!(names.contains(&"release.cbor".to_string()), "{names:?}");
	assert!(names.contains(&"entry.cbor".to_string()), "{names:?}");
	assert!(names.contains(&"verify.md".to_string()), "{names:?}");

	let mut instructions = String::new();
	archive
		.by_name("verify.md")
		.expect("verify.md")
		.read_to_string(&mut instructions)
		.expect("read");
	assert!(instructions.contains(&project_id));
	assert!(instructions.contains("moraine-verify object --kind release"));
	for line in instructions
		.lines()
		.filter(|line| line.trim_start().starts_with("moraine-verify"))
	{
		assert!(line.contains("--root"), "`{line}` would refuse to verify without a root");
	}

	let missing = axum::http::Request::get(format!("/v1/projects/{project_id}/releases/{}/bundle", "00".repeat(32)))
		.body(Body::empty())
		.expect("request");
	let response = application.oneshot(missing).await.expect("response");
	assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
