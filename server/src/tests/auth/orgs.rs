#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use axum::Router;
	use axum::body::{Body, to_bytes};
	use axum::http::{Method, StatusCode, header};
	use axum::response::Response;
	use tower::ServiceExt;

	use crate::blob::BlobStore;
	use crate::capability::Capability;
	use crate::db::MetadataStore;
	use crate::routes::AppState;

	async fn app() -> (Router, tempfile::TempDir) {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = Arc::new(BlobStore::new(directory.path()).await.expect("blob store"));
		let metadata = Arc::new(
			MetadataStore::open(directory.path().join("metadata.sqlite"))
				.await
				.expect("metadata"),
		);
		let config = crate::config::Config {
			bind: "127.0.0.1:0".parse().expect("addr"),
			data_dir: directory.path().to_path_buf(),
			max_artifact_bytes: 1024,
			registration: crate::config::Registration::Open,
			allow_insecure_federation_local: false,
			publishing: crate::config::Publishing::Open,
			..crate::config::Config::default()
		};
		let state = AppState {
			store,
			metadata,
			capability: Arc::new(Capability::discover(&config)),
			branding: std::sync::Arc::new(crate::instance::BrandingSource::default()),
			login_limiter: std::sync::Arc::new(crate::auth::LoginLimiter::new()),
			metrics: std::sync::Arc::new(crate::ops::metrics::Metrics::new()),
			rate_limiter: std::sync::Arc::new(crate::auth::ratelimit::RateLimiter::new()),
			web_dir: None,
		};
		(crate::routes::router(state), directory)
	}

	async fn login(app: &Router, email: &str) -> (String, String) {
		let credentials = serde_json::json!({ "email": email, "password": "correct horse battery" }).to_string();
		let register = axum::http::Request::post("/v1/auth/register")
			.header(header::CONTENT_TYPE, "application/json")
			.body(Body::from(credentials.clone()))
			.expect("request");
		let response = app.clone().oneshot(register).await.expect("response");
		let user_id = body_json(response).await["user_id"].as_str().expect("user id").to_string();
		let session = axum::http::Request::post("/v1/auth/session")
			.header(header::CONTENT_TYPE, "application/json")
			.body(Body::from(credentials))
			.expect("request");
		let response = app.clone().oneshot(session).await.expect("response");
		let session_token = set_cookie(&response, "moraine_session");
		let csrf = set_cookie(&response, "moraine_csrf");
		(user_id, format!("moraine_session={session_token}; moraine_csrf={csrf}"))
	}

	fn set_cookie(response: &Response, name: &str) -> String {
		response
			.headers()
			.get_all(header::SET_COOKIE)
			.iter()
			.find_map(|value| {
				let cookie = value.to_str().ok()?;
				cookie
					.split(';')
					.next()?
					.strip_prefix(&format!("{name}="))
					.map(str::to_string)
			})
			.unwrap_or_default()
	}

	fn csrf_of(cookie: &str) -> String {
		cookie
			.split(';')
			.find_map(|part| part.trim().strip_prefix("moraine_csrf="))
			.unwrap_or_default()
			.to_string()
	}

	async fn body_json(response: Response) -> serde_json::Value {
		let bytes = to_bytes(response.into_body(), 64 * 1024).await.expect("body");
		serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
	}

	fn write(path: &str, cookie: &str, body: serde_json::Value) -> axum::http::Request<Body> {
		axum::http::Request::builder()
			.method(Method::POST)
			.uri(path)
			.header(header::CONTENT_TYPE, "application/json")
			.header(header::COOKIE, cookie)
			.header("x-csrf-token", csrf_of(cookie))
			.body(Body::from(body.to_string()))
			.expect("request")
	}

	fn read(path: &str, cookie: &str) -> axum::http::Request<Body> {
		axum::http::Request::builder()
			.method(Method::GET)
			.uri(path)
			.header(header::COOKIE, cookie)
			.body(Body::empty())
			.expect("request")
	}

	#[tokio::test]
	async fn lists_the_orgs_an_account_belongs_to() {
		let (application, _directory) = app().await;
		let (_, owner_cookie) = login(&application, "owner@example.org").await;
		let (_, outsider_cookie) = login(&application, "outsider@example.org").await;
		let create = write(
			"/v1/orgs",
			&owner_cookie,
			serde_json::json!({ "handle": "cleanroom", "display_name": "Cleanroom" }),
		);
		let response = application.clone().oneshot(create).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let response = application
			.clone()
			.oneshot(read("/v1/orgs", &owner_cookie))
			.await
			.expect("response");
		let orgs = body_json(response).await;
		assert_eq!(orgs.as_array().expect("orgs").len(), 1);
		assert_eq!(orgs[0]["handle"], "cleanroom");
		assert_eq!(orgs[0]["display_name"], "Cleanroom");
		assert_eq!(orgs[0]["role"], "owner");

		let response = application
			.oneshot(read("/v1/orgs", &outsider_cookie))
			.await
			.expect("response");
		let orgs = body_json(response).await;
		assert!(orgs.as_array().expect("orgs").is_empty());
	}

	#[tokio::test]
	async fn owner_manages_members_and_last_owner_is_protected() {
		let (application, _directory) = app().await;
		let (owner_id, owner_cookie) = login(&application, "owner@example.org").await;
		let (member_id, member_cookie) = login(&application, "member@example.org").await;

		let create = write(
			"/v1/orgs",
			&owner_cookie,
			serde_json::json!({ "handle": "cleanroom", "display_name": "Cleanroom" }),
		);
		let response = application.clone().oneshot(create).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let add = write(
			"/v1/orgs/cleanroom/members",
			&owner_cookie,
			serde_json::json!({ "email": "member@example.org", "role": "member" }),
		);
		let response = application.clone().oneshot(add).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let members = application
			.clone()
			.oneshot(read("/v1/orgs/cleanroom/members", &member_cookie))
			.await
			.expect("response");
		let list = body_json(members).await;
		assert_eq!(list.as_array().expect("members").len(), 2);

		let member_adds = write(
			"/v1/orgs/cleanroom/members",
			&member_cookie,
			serde_json::json!({ "email": "owner@example.org", "role": "admin" }),
		);
		let response = application.clone().oneshot(member_adds).await.expect("response");
		assert_eq!(response.status(), StatusCode::FORBIDDEN);

		let remove_owner = axum::http::Request::builder()
			.method(Method::DELETE)
			.uri(format!("/v1/orgs/cleanroom/members/{owner_id}"))
			.header(header::COOKIE, &owner_cookie)
			.header("x-csrf-token", csrf_of(&owner_cookie))
			.body(Body::empty())
			.expect("request");
		let response = application.clone().oneshot(remove_owner).await.expect("response");
		assert_eq!(response.status(), StatusCode::CONFLICT);

		let member_removes_self = axum::http::Request::builder()
			.method(Method::DELETE)
			.uri(format!("/v1/orgs/cleanroom/members/{member_id}"))
			.header(header::COOKIE, &owner_cookie)
			.header("x-csrf-token", csrf_of(&owner_cookie))
			.body(Body::empty())
			.expect("request");
		let response = application.clone().oneshot(member_removes_self).await.expect("response");
		assert_eq!(response.status(), StatusCode::NO_CONTENT);

		let promote = write(
			"/v1/orgs/cleanroom/members",
			&owner_cookie,
			serde_json::json!({ "email": "member@example.org", "role": "admin" }),
		);
		let response = application.clone().oneshot(promote).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let admin_demotes_owner = write(
			"/v1/orgs/cleanroom/members",
			&member_cookie,
			serde_json::json!({ "email": "owner@example.org", "role": "member" }),
		);
		let response = application.clone().oneshot(admin_demotes_owner).await.expect("response");
		assert_eq!(response.status(), StatusCode::FORBIDDEN, "an admin cannot demote an owner");

		let owner_demotes_self = write(
			"/v1/orgs/cleanroom/members",
			&owner_cookie,
			serde_json::json!({ "email": "owner@example.org", "role": "member" }),
		);
		let response = application.oneshot(owner_demotes_self).await.expect("response");
		assert_eq!(response.status(), StatusCode::BAD_REQUEST, "the last owner cannot be demoted");
	}

	#[tokio::test]
	async fn nested_teams_must_share_the_org() {
		let (application, _directory) = app().await;
		let (_owner_id, owner_cookie) = login(&application, "owner@example.org").await;
		application
			.clone()
			.oneshot(write(
				"/v1/orgs",
				&owner_cookie,
				serde_json::json!({ "handle": "team-org", "display_name": "Team Org" }),
			))
			.await
			.expect("response");
		let parent = body_json(
			application
				.clone()
				.oneshot(write(
					"/v1/orgs/team-org/teams",
					&owner_cookie,
					serde_json::json!({ "display_name": "Maintainers" }),
				))
				.await
				.expect("response"),
		)
		.await;
		let parent_id = parent["id"].as_str().expect("team id").to_string();

		let child = write(
			"/v1/orgs/team-org/teams",
			&owner_cookie,
			serde_json::json!({ "display_name": "Reviewers", "parent_team_id": parent_id }),
		);
		let response = application.clone().oneshot(child).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);
		let child_id = body_json(response).await["id"].as_str().expect("team id").to_string();

		let reparent = axum::http::Request::builder()
			.method(Method::PATCH)
			.uri(format!("/v1/orgs/team-org/teams/{parent_id}"))
			.header(header::CONTENT_TYPE, "application/json")
			.header(header::COOKIE, &owner_cookie)
			.header("x-csrf-token", csrf_of(&owner_cookie))
			.body(Body::from(serde_json::json!({ "parent_team_id": null }).to_string()))
			.expect("request");
		let response = application.clone().oneshot(reparent).await.expect("response");
		assert_eq!(response.status(), StatusCode::OK);
		let view = body_json(response).await;
		assert!(view["parent_team_id"].is_null());

		let cycle = axum::http::Request::builder()
			.method(Method::PATCH)
			.uri(format!("/v1/orgs/team-org/teams/{parent_id}"))
			.header(header::CONTENT_TYPE, "application/json")
			.header(header::COOKIE, &owner_cookie)
			.header("x-csrf-token", csrf_of(&owner_cookie))
			.body(Body::from(serde_json::json!({ "parent_team_id": child_id }).to_string()))
			.expect("request");
		let response = application.oneshot(cycle).await.expect("response");
		assert_eq!(response.status(), StatusCode::BAD_REQUEST);
	}
}
