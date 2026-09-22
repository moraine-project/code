use super::*;

#[derive(Serialize)]
pub struct SyncReport {
	project_id: String,
	applied: usize,
	head_seq: i64,
}

pub(super) async fn sync_handler(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Json(request): Json<SyncRequest>,
) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let home_url = request.home_url.trim_end_matches('/').to_string();
	match sync(&state, &home_url, &request.project_id).await {
		Ok(report) => Json(report).into_response(),
		Err(error) => {
			let status = match error {
				FederationError::InvalidUrl(_) => StatusCode::BAD_REQUEST,
				FederationError::Rejected(_) => StatusCode::CONFLICT,
				FederationError::Verify(_) | FederationError::Decode(_) | FederationError::Fork(_) => {
					StatusCode::BAD_GATEWAY
				}
				_ => StatusCode::BAD_GATEWAY,
			};
			state.metrics.record_federation_failure(&error);
			(status, error.to_string()).into_response()
		}
	}
}

pub(super) async fn list_subscriptions(State(state): State<AppState>, user: AuthenticatedUser) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	match state.metadata.subscriptions().await {
		Ok(rows) => {
			let view: Vec<SubscriptionView> = rows
				.into_iter()
				.map(|row| SubscriptionView {
					home_url: row.home_url,
					project_id: row.project_id,
					cursor_seq: row.cursor_seq,
					lag_entries: (row.remote_head_seq - row.cursor_seq).max(0),
					resets: row.reset_count,
					status: row.status,
					updated_at: row.updated_at,
				})
				.collect();
			Json(view).into_response()
		}
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
pub(super) struct ResetQuery {
	home_url: String,
	project_id: String,
	#[serde(default)]
	cursor: Option<i64>,
}

pub(super) async fn reset_subscription(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Query(query): Query<ResetQuery>,
) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let cursor = query.cursor.unwrap_or(0).max(0);
	match state
		.metadata
		.reset_subscription(&query.home_url, &query.project_id, cursor, now())
		.await
	{
		Ok(true) => Json(serde_json::json!({ "cursor_seq": cursor })).into_response(),
		Ok(false) => (StatusCode::NOT_FOUND, "not subscribed to that project").into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Deserialize)]
pub(super) struct UnsubscribeQuery {
	home_url: String,
	project_id: String,
}

pub(super) async fn unsubscribe(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Query(query): Query<UnsubscribeQuery>,
) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	match state.metadata.remove_subscription(&query.home_url, &query.project_id).await {
		Ok(true) => StatusCode::NO_CONTENT.into_response(),
		Ok(false) => (StatusCode::NOT_FOUND, "not subscribed to that project").into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Serialize)]
struct SubscriptionView {
	home_url: String,
	project_id: String,
	cursor_seq: i64,
	lag_entries: i64,
	resets: i64,
	status: String,
	updated_at: i64,
}

pub(crate) async fn sync(state: &AppState, home_url: &str, project_id: &str) -> Result<SyncReport, FederationError> {
	let client = HomeClient::new(
		home_url,
		state.capability.allow_insecure_federation_local,
		state.capability.max_response_bytes,
		&state.capability.tls_extra_roots,
	)
	.await?;
	let summary = client
		.get_json::<ProjectSummary>(&format!("/v1/projects/{project_id}"))
		.await?;
	if summary.project_id != project_id {
		return Err(FederationError::Verify("home returned a different project id".to_string()));
	}

	let genesis_wire = client
		.get_bytes(&format!("/v1/objects/{}", hex_of(&summary.genesis)?))
		.await?;
	let (root, genesis) =
		verify::verify_genesis(&genesis_wire).map_err(|error| FederationError::Verify(error.to_string()))?;
	if genesis.id != project_id {
		return Err(FederationError::Verify(
			"genesis id does not match the requested project".to_string(),
		));
	}

	let mut cursor = match state.metadata.subscription(home_url, project_id).await.map_err(storage)? {
		Some(subscription) => subscription.cursor_seq,
		None => 0,
	};

	match state.metadata.project(project_id).await.map_err(storage)? {
		Some(existing) => {
			if existing.genesis_digest != genesis.digest.to_vec() {
				return Err(FederationError::Verify("local project has a different genesis".to_string()));
			}
			if existing.head_seq > summary.head_seq {
				return Err(FederationError::Fork(format!(
					"its head moved backwards from {} to {}",
					existing.head_seq, summary.head_seq
				)));
			}
			if existing.head_seq == summary.head_seq
				&& existing.head_seq > 0
				&& let (Some(local), Some(remote)) = (existing.head_digest.as_deref(), summary.head_entry.as_deref())
				&& registry::id_for(local) != remote
			{
				return Err(FederationError::Fork(format!(
					"it serves a different entry at sequence {}",
					existing.head_seq
				)));
			}
		}
		None => {
			state
				.metadata
				.put_object(&registry::stored(&genesis))
				.await
				.map_err(storage)?;
			state
				.metadata
				.create_project(project_id, &genesis.digest)
				.await
				.map_err(storage)?;
		}
	}
	state
		.metadata
		.upsert_subscription(home_url, project_id, "active", now())
		.await
		.map_err(storage)?;

	let mut applied = 0;
	let mut head_seq;
	let mut pages = 0;
	let mut observed_head: Option<(i64, String)> = None;
	loop {
		let page = client
			.get_json::<FeedPage>(&format!("/v1/projects/{project_id}/feed?after={cursor}&limit=100"))
			.await?;
		head_seq = page.head_seq;
		if head_seq < cursor {
			return Err(FederationError::Rejected(format!(
				"the home's feed went backwards from {cursor} to {head_seq}"
			)));
		}
		if page.entries.len() > state.capability.max_feed_page_entries as usize {
			return Err(FederationError::Verify(
				"the home returned more entries in a page than the requested limit".to_string(),
			));
		}
		let Some(last) = page.entries.last() else {
			break;
		};
		for entry in &page.entries {
			if let Some(kind) = object_kind_for_event(&entry.kind) {
				let wire = client.get_bytes(&format!("/v1/objects/{}", hex_of(&entry.object)?)).await?;
				let delegations = load_delegations(state, project_id, &root)
					.await
					.map_err(|error| rejected(*error))?;
				let object = verify::verify_object_authorized(kind, &wire, &root, &delegations, now())
					.map_err(|error| FederationError::Verify(error.to_string()))?;
				store_synced_object(state, &object).await?;
				if kind == ObjectKind::Release
					&& let Some(digest) = release_changelog_digest(&object.payload_bytes)
				{
					let wire = client.get_bytes(&format!("/v1/objects/{}", hex::encode(digest))).await?;
					let changelog =
						verify::verify_object_authorized(ObjectKind::Changelog, &wire, &root, &delegations, now())
							.map_err(|error| FederationError::Verify(error.to_string()))?;
					store_synced_object(state, &changelog).await?;
				}
			}
			let entry_wire = client.get_bytes(&format!("/v1/objects/{}", hex_of(&entry.entry)?)).await?;
			registry::ingest_feed(state, project_id, &entry_wire)
				.await
				.map_err(|error| rejected(*error))?;
			applied += 1;
		}
		if last.seq == head_seq {
			observed_head = Some((head_seq, last.entry.clone()));
		}
		if last.seq <= cursor {
			break;
		}
		state
			.metadata
			.set_subscription_cursor(home_url, project_id, last.seq, head_seq, "active", now())
			.await
			.map_err(storage)?;
		cursor = last.seq;
		if last.seq >= head_seq {
			break;
		}
		pages += 1;
		if pages >= state.capability.max_sync_pages as usize {
			return Err(FederationError::Verify(
				"the home feed is longer than one sync will follow".to_string(),
			));
		}
	}

	if let Some((sequence, entry)) = observed_head {
		state
			.metadata
			.record_witness_observation(project_id, home_url, sequence, &entry, now())
			.await
			.map_err(storage)?;
	}

	Ok(SyncReport {
		project_id: project_id.to_string(),
		applied,
		head_seq,
	})
}

fn release_changelog_digest(payload: &[u8]) -> Option<Vec<u8>> {
	let moraine_model::release::ReleaseObject::Release(release) =
		moraine_model::release::ReleaseObject::from_canonical_bytes(payload).ok()?
	else {
		return None;
	};
	release.changelog_digest
}

fn object_kind_for_event(event: &str) -> Option<ObjectKind> {
	Some(match event {
		"release-published" | "release-withdrawn" => ObjectKind::Release,
		"profile-updated" => ObjectKind::Profile,
		"key-changed" | "migration" | "recovery" | "ownership-transferred" => ObjectKind::Delegation,
		"advisory" => ObjectKind::Advisory,
		_ => return None,
	})
}

pub(super) async fn sync_loader_releases(
	state: &AppState,
	client: &HomeClient,
	loader_id: &str,
	root: &moraine_model::trust::RootSet,
) -> Result<(), FederationError> {
	let releases = client
		.get_json::<Vec<LoaderReleaseSummary>>(&format!("/v1/loaders/{loader_id}/releases"))
		.await?;
	if releases.len() > state.capability.max_sync_entries as usize {
		return Err(FederationError::Verify(
			"the home lists more loader releases than one sync will follow".to_string(),
		));
	}
	for release in releases {
		let wire = client
			.get_bytes(&format!("/v1/objects/{}", hex_of(&release.release)?))
			.await?;
		let object = verify::verify_object(ObjectKind::LoaderDef, &wire, root)
			.map_err(|error| FederationError::Verify(error.to_string()))?;
		state.metadata.put_object(&registry::stored(&object)).await.map_err(storage)?;
		let Ok(moraine_model::definition::LoaderObject::Release(payload)) =
			moraine_model::definition::LoaderObject::from_canonical_bytes(&object.payload_bytes)
		else {
			continue;
		};
		state
			.metadata
			.index_loader_release(&payload.loader_id, &payload.version_id, &object.digest, payload.declared_time)
			.await
			.map_err(storage)?;
	}
	Ok(())
}

async fn store_synced_object(state: &AppState, object: &verify::VerifiedObject) -> Result<(), FederationError> {
	registry::store_object_record(state, object)
		.await
		.map_err(|error| match error {
			sqlx::Error::Protocol(message) => FederationError::Verify(message),
			other => storage(other),
		})
}
