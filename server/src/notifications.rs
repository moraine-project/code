use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::auth::AuthenticatedUser;
use crate::routes::AppState;
use crate::store::MetadataStore;

#[derive(Debug, Clone)]
pub struct NotificationRow {
	pub id: String,
	pub project_id: String,
	pub event_kind: String,
	pub object_digest: Option<Vec<u8>>,
	pub feed_seq: Option<i64>,
	pub created_at: i64,
	pub read_at: Option<i64>,
}

impl MetadataStore {
	pub async fn follow(&self, user_id: &str, project_id: &str, created_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT OR IGNORE INTO follows (user_id, project_id, created_at) VALUES (?1, ?2, ?3)")
			.bind(user_id)
			.bind(project_id)
			.bind(created_at)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn unfollow(&self, user_id: &str, project_id: &str) -> Result<bool, sqlx::Error> {
		let result = sqlx::query("DELETE FROM follows WHERE user_id = ?1 AND project_id = ?2")
			.bind(user_id)
			.bind(project_id)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn follows(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
		let rows = sqlx::query("SELECT project_id FROM follows WHERE user_id = ?1 ORDER BY created_at")
			.bind(user_id)
			.fetch_all(&self.pool)
			.await?;
		Ok(rows.into_iter().map(|row| row.get("project_id")).collect())
	}

	pub async fn followers(&self, project_id: &str) -> Result<Vec<String>, sqlx::Error> {
		let rows = sqlx::query("SELECT user_id FROM follows WHERE project_id = ?1")
			.bind(project_id)
			.fetch_all(&self.pool)
			.await?;
		Ok(rows.into_iter().map(|row| row.get("user_id")).collect())
	}

	pub async fn insert_notification(&self, notification: &NotificationRow, user_id: &str) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO notifications (id, user_id, project_id, event_kind, object_digest, feed_seq, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
		)
		.bind(&notification.id)
		.bind(user_id)
		.bind(&notification.project_id)
		.bind(&notification.event_kind)
		.bind(&notification.object_digest)
		.bind(notification.feed_seq)
		.bind(notification.created_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn notifications(
		&self,
		user_id: &str,
		unread_only: bool,
		limit: i64,
	) -> Result<Vec<NotificationRow>, sqlx::Error> {
		let sql = if unread_only {
			"SELECT id, project_id, event_kind, object_digest, feed_seq, created_at, read_at FROM notifications WHERE user_id = ?1 AND read_at IS NULL ORDER BY created_at DESC LIMIT ?2"
		} else {
			"SELECT id, project_id, event_kind, object_digest, feed_seq, created_at, read_at FROM notifications WHERE user_id = ?1 ORDER BY created_at DESC LIMIT ?2"
		};
		let rows = sqlx::query(sql).bind(user_id).bind(limit).fetch_all(&self.pool).await?;
		Ok(rows
			.into_iter()
			.map(|row| NotificationRow {
				id: row.get("id"),
				project_id: row.get("project_id"),
				event_kind: row.get("event_kind"),
				object_digest: row.get("object_digest"),
				feed_seq: row.get("feed_seq"),
				created_at: row.get("created_at"),
				read_at: row.get("read_at"),
			})
			.collect())
	}

	pub async fn mark_notification_read(&self, user_id: &str, id: &str, read_at: i64) -> Result<bool, sqlx::Error> {
		let result = sqlx::query("UPDATE notifications SET read_at = ?1 WHERE id = ?2 AND user_id = ?3 AND read_at IS NULL")
			.bind(read_at)
			.bind(id)
			.bind(user_id)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected() == 1)
	}

	pub async fn mark_all_notifications_read(&self, user_id: &str, read_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE notifications SET read_at = ?1 WHERE user_id = ?2 AND read_at IS NULL")
			.bind(read_at)
			.bind(user_id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn prune_notifications(&self, before: i64) -> Result<u64, sqlx::Error> {
		let result = sqlx::query("DELETE FROM notifications WHERE created_at < ?1")
			.bind(before)
			.execute(&self.pool)
			.await?;
		Ok(result.rows_affected())
	}
}

/// Records a notification for every follower of a project. Notifications are a
/// local convenience derived from the validated feed; the feed stays the
/// source of truth and a notification is reproducible from it.
pub(crate) async fn notify_followers(
	state: &AppState,
	project_id: &str,
	event_kind: &str,
	object_digest: &[u8],
	feed_seq: i64,
) -> Result<(), sqlx::Error> {
	let followers = state.metadata.followers(project_id).await?;
	for user_id in followers {
		let notification = NotificationRow {
			id: new_id(),
			project_id: project_id.to_string(),
			event_kind: event_kind.to_string(),
			object_digest: Some(object_digest.to_vec()),
			feed_seq: Some(feed_seq),
			created_at: now(),
			read_at: None,
		};
		state.metadata.insert_notification(&notification, &user_id).await?;
	}
	Ok(())
}

pub fn routes() -> Router<AppState> {
	Router::new()
		.route("/v1/follows", get(list_follows))
		.route(
			"/v1/follows/{project_id}",
			post(follow).delete(axum::routing::delete(unfollow)),
		)
		.route("/v1/notifications", get(list_notifications))
		.route("/v1/notifications/read-all", post(read_all))
		.route("/v1/notifications/{id}/read", post(read_one))
}

#[derive(Serialize)]
struct NotificationView {
	id: String,
	project_id: String,
	event_kind: String,
	object: Option<String>,
	feed_seq: Option<i64>,
	created_at: i64,
	read: bool,
}

async fn follow(State(state): State<AppState>, user: AuthenticatedUser, Path(project_id): Path<String>) -> Response {
	match state.metadata.project(&project_id).await {
		Ok(Some(_)) => {}
		Ok(None) => return (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => return storage_error(error),
	}
	match state.metadata.follow(&user.user_id, &project_id, now()).await {
		Ok(()) => (StatusCode::CREATED, Json(serde_json::json!({ "project_id": project_id }))).into_response(),
		Err(error) => storage_error(error),
	}
}

async fn unfollow(State(state): State<AppState>, user: AuthenticatedUser, Path(project_id): Path<String>) -> Response {
	match state.metadata.unfollow(&user.user_id, &project_id).await {
		Ok(true) => StatusCode::NO_CONTENT.into_response(),
		Ok(false) => (StatusCode::NOT_FOUND, "not following").into_response(),
		Err(error) => storage_error(error),
	}
}

async fn list_follows(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	match state.metadata.follows(&user.user_id).await {
		Ok(projects) => Json(projects).into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
struct NotificationQuery {
	#[serde(default)]
	unread: Option<bool>,
}

async fn list_notifications(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Query(query): Query<NotificationQuery>,
) -> Response {
	match state
		.metadata
		.notifications(&user.user_id, query.unread.unwrap_or(false), 100)
		.await
	{
		Ok(rows) => Json(
			rows.into_iter()
				.map(|row| NotificationView {
					id: row.id,
					project_id: row.project_id,
					event_kind: row.event_kind,
					object: row.object_digest.map(|digest| format!("gd:sha256:{}", hex::encode(digest))),
					feed_seq: row.feed_seq,
					created_at: row.created_at,
					read: row.read_at.is_some(),
				})
				.collect::<Vec<_>>(),
		)
		.into_response(),
		Err(error) => storage_error(error),
	}
}

async fn read_one(State(state): State<AppState>, user: AuthenticatedUser, Path(id): Path<String>) -> Response {
	match state.metadata.mark_notification_read(&user.user_id, &id, now()).await {
		Ok(true) => StatusCode::NO_CONTENT.into_response(),
		Ok(false) => (StatusCode::NOT_FOUND, "no such notification").into_response(),
		Err(error) => storage_error(error),
	}
}

async fn read_all(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	match state.metadata.mark_all_notifications_read(&user.user_id, now()).await {
		Ok(()) => StatusCode::NO_CONTENT.into_response(),
		Err(error) => storage_error(error),
	}
}

fn new_id() -> String {
	let mut bytes = [0u8; 16];
	if getrandom::fill(&mut bytes).is_err() {
		panic!("operating system randomness is unavailable");
	}
	hex::encode(bytes)
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

fn storage_error(error: sqlx::Error) -> Response {
	tracing::error!(%error, "notification store failed");
	(StatusCode::INTERNAL_SERVER_ERROR, "storage error").into_response()
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use axum::body::{Body, to_bytes};
	use axum::http::header;
	use moraine_crypto::{ObjectKind as Kind, SigningKey, object_id};
	use moraine_model::artifact::Artifact;
	use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
	use moraine_model::feed::FeedEntry;
	use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
	use moraine_model::release::ReleasePayload;
	use moraine_model::signed::sign_payload;
	use tower::ServiceExt;

	use super::*;
	use crate::blob::BlobStore;
	use crate::capability::Capability;

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
			max_feed_page_entries: 100,
			allow_insecure_federation_local: false,
			publishing: crate::config::Publishing::Open,
			web_dir: None,
		};
		let state = AppState {
			store,
			metadata,
			capability: Arc::new(Capability::discover(&config)),
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
		app.clone().oneshot(register).await.expect("response");
		let session = axum::http::Request::post("/v1/auth/session")
			.header(header::CONTENT_TYPE, "application/json")
			.body(Body::from(credentials))
			.expect("request");
		let response = app.clone().oneshot(session).await.expect("response");
		let token = set_cookie(&response, "moraine_session");
		let csrf = set_cookie(&response, "moraine_csrf");
		(format!("moraine_session={token}; moraine_csrf={csrf}"), csrf)
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

	async fn body_json(response: Response) -> serde_json::Value {
		let bytes = to_bytes(response.into_body(), 64 * 1024).await.expect("body");
		serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
	}

	fn genesis_wire(signer: &SigningKey) -> Vec<u8> {
		let genesis = Genesis {
			protocol: 1,
			kind: GenesisKind::Project,
			nonce: vec![0x11; 16],
			roots: vec![RootKey::from_public_key(signer.verifying_key().to_bytes().to_vec()).expect("root")],
			threshold: 1,
			authorized_kinds: vec!["delegation".to_string(), "release".to_string(), "profile".to_string()],
			home_hint: None,
			contacts: None,
			created_at: 1_760_000_000,
		};
		sign_payload(Kind::Genesis, &genesis, &[signer]).wire_bytes()
	}

	fn release_wire(signer: &SigningKey, project_id: &str) -> ([u8; 32], Vec<u8>) {
		let release = ReleasePayload {
			protocol: 1,
			project_id: project_id.to_string(),
			game_id: "gd:sha256:game".to_string(),
			release_nonce: vec![0x42; 16],
			human_version: "1.0.0".to_string(),
			channel: "release".to_string(),
			kind: "mod".to_string(),
			declared_time: 1_760_000_000,
			compatibility: vec![Compatibility {
				game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
				loader_id: None,
				loader_version_predicate: None,
				side: Side::Both,
				runtime_predicate: None,
				os_predicate: None,
				arch_predicate: None,
			}],
			artifacts: vec![Artifact {
				digest: vec![0xAB; 32],
				size: 10,
				media_type: "application/java-archive".to_string(),
				filename: "example.jar".to_string(),
				is_primary: true,
				os_predicate: None,
				arch_predicate: None,
			}],
			dependencies: Vec::new(),
			source_reference: None,
			changelog_digest: None,
			license_expression: None,
			rights: None,
			sbom_digest: None,
			minimum_verifier_version: 1,
			critical_extensions: Vec::new(),
		};
		let signed = sign_payload(Kind::Release, &release, &[signer]);
		(object_id(Kind::Release, &signed.payload_bytes), signed.wire_bytes())
	}

	fn feed_wire(
		signer: &SigningKey,
		project_id: &str,
		sequence: u64,
		previous: Option<[u8; 32]>,
		object: [u8; 32],
	) -> Vec<u8> {
		let entry = FeedEntry {
			protocol: 1,
			project_id: project_id.to_string(),
			sequence,
			previous: previous.map(|digest| digest.to_vec()),
			kind: "release-published".to_string(),
			object_digest: object.to_vec(),
			declared_at: 1_760_000_000 + sequence as i64,
		};
		sign_payload(Kind::FeedEntry, &entry, &[signer]).wire_bytes()
	}

	#[tokio::test]
	async fn followers_receive_notifications_and_can_read_them() {
		let (application, _directory) = app().await;
		let signer = SigningKey::from_seed(&[31u8; 32]);
		let (cookie, csrf) = login(&application, "fan@example.org").await;

		let request = axum::http::Request::post("/v1/projects")
			.body(Body::from(genesis_wire(&signer)))
			.expect("request");
		let response = application.clone().oneshot(request).await.expect("response");
		let project_id = body_json(response).await["project_id"]
			.as_str()
			.expect("project id")
			.to_string();

		let (release_digest, release) = release_wire(&signer, &project_id);
		let request = axum::http::Request::post(format!("/v1/projects/{project_id}/objects/release"))
			.body(Body::from(release))
			.expect("request");
		application.clone().oneshot(request).await.expect("response");

		let follow = axum::http::Request::builder()
			.method("POST")
			.uri(format!("/v1/follows/{project_id}"))
			.header(header::COOKIE, &cookie)
			.header("x-csrf-token", &csrf)
			.body(Body::empty())
			.expect("request");
		let response = application.clone().oneshot(follow).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let request = axum::http::Request::post(format!("/v1/projects/{project_id}/feed"))
			.body(Body::from(feed_wire(&signer, &project_id, 1, None, release_digest)))
			.expect("request");
		let response = application.clone().oneshot(request).await.expect("response");
		assert_eq!(response.status(), StatusCode::CREATED);

		let listing = axum::http::Request::get("/v1/notifications?unread=true")
			.header(header::COOKIE, &cookie)
			.body(Body::empty())
			.expect("request");
		let response = application.clone().oneshot(listing).await.expect("response");
		let list = body_json(response).await;
		assert_eq!(list.as_array().expect("notifications").len(), 1);
		assert_eq!(list[0]["event_kind"], "release-published");
		let notification_id = list[0]["id"].as_str().expect("id").to_string();

		let read = axum::http::Request::builder()
			.method("POST")
			.uri(format!("/v1/notifications/{notification_id}/read"))
			.header(header::COOKIE, &cookie)
			.header("x-csrf-token", &csrf)
			.body(Body::empty())
			.expect("request");
		let response = application.clone().oneshot(read).await.expect("response");
		assert_eq!(response.status(), StatusCode::NO_CONTENT);

		let listing = axum::http::Request::get("/v1/notifications?unread=true")
			.header(header::COOKIE, &cookie)
			.body(Body::empty())
			.expect("request");
		let response = application.oneshot(listing).await.expect("response");
		let list = body_json(response).await;
		assert!(list.as_array().expect("notifications").is_empty());
	}
}

#[cfg(test)]
mod prune_tests {
	use super::*;

	#[tokio::test]
	async fn prunes_notifications_past_retention() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open(directory.path().join("metadata.sqlite"))
			.await
			.expect("store");
		let old = NotificationRow {
			id: "old".to_string(),
			project_id: "p".to_string(),
			event_kind: "release-published".to_string(),
			object_digest: None,
			feed_seq: Some(1),
			created_at: 1_000,
			read_at: None,
		};
		let fresh = NotificationRow {
			id: "fresh".to_string(),
			created_at: now(),
			..old.clone()
		};
		store.insert_notification(&old, "u").await.expect("old");
		store.insert_notification(&fresh, "u").await.expect("fresh");

		let pruned = store.prune_notifications(now() - 90 * 86_400).await.expect("prune");
		assert_eq!(pruned, 1);
		let remaining = store.notifications("u", false, 10).await.expect("list");
		assert_eq!(remaining.len(), 1);
		assert_eq!(remaining[0].id, "fresh");
	}
}
