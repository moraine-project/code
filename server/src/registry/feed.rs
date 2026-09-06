use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use moraine_model::Canonical;
use moraine_model::feed::FeedEntry;
use serde::{Deserialize, Serialize};

use super::{id_for, storage_error};
use crate::registry::views::describe_stored;
use crate::routes::AppState;

#[derive(Serialize)]
struct FeedEntryView {
	seq: i64,
	kind: String,
	title: Option<String>,
	release: Option<crate::registry::views::ReleaseSummary>,
	object: String,
	entry: String,
	declared_at: i64,
	previous: Option<String>,
}

#[derive(Serialize)]
struct FeedPage {
	project_id: String,
	head_seq: i64,
	entries: Vec<FeedEntryView>,
	next: Option<i64>,
	truncated: bool,
}

#[derive(Deserialize)]
pub(crate) struct FeedQuery {
	#[serde(default)]
	after: i64,
	limit: Option<i64>,
	#[serde(default)]
	game_version: Option<String>,
	#[serde(default)]
	loader: Option<String>,
	#[serde(default)]
	loader_version: Option<String>,
	#[serde(default)]
	runtime: Option<String>,
	#[serde(default)]
	runtime_version: Option<String>,
}

pub(crate) async fn page(State(state): State<AppState>, Path(id): Path<String>, Query(query): Query<FeedQuery>) -> Response {
	let project = match state.metadata.project(&id).await {
		Ok(Some(project)) => project,
		Ok(None) => return (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => return storage_error(error),
	};
	let limit = query
		.limit
		.unwrap_or(state.capability.max_feed_page_entries as i64)
		.clamp(1, state.capability.max_feed_page_entries as i64);
	let mut entries = Vec::with_capacity(limit as usize);
	let mut catalog: Option<Option<moraine_model::version::VersionCatalog>> = None;
	let mut loader_scheme: Option<Option<moraine_model::version::OrderingScheme>> = None;
	let mut runtime_scheme: Option<Option<moraine_model::version::OrderingScheme>> = None;
	let mut scanned = query.after;
	let mut pages = 0;
	loop {
		let rows = match state.metadata.feed_after(&id, scanned, limit).await {
			Ok(rows) => rows,
			Err(error) => return storage_error(error),
		};
		if rows.is_empty() {
			break;
		}
		let fetched = rows.len() as i64;
		scanned = rows.last().map(|row| row.seq).unwrap_or(scanned);
		for row in &rows {
			if entries.len() as i64 >= limit {
				break;
			}
			let declared_at = FeedEntry::from_canonical_bytes(&row.payload)
				.map(|entry| entry.declared_at)
				.unwrap_or(0);
			let object = match state.metadata.object(&row.object_digest).await {
				Ok(Some(object)) => Some(object),
				_ => None,
			};
			if let Some(object) = object.as_ref()
				&& object.kind == "release"
			{
				if let Some(version) = query.game_version.as_deref() {
					let resolved = match &catalog {
						Some(cached) => cached.clone(),
						None => {
							let value = crate::registry::compatibility::game_catalog(&state, object).await;
							catalog = Some(value.clone());
							value
						}
					};
					if !crate::registry::compatibility::release_matches_game_version(object, version, resolved.as_ref()) {
						continue;
					}
				}
				if let Some(loader) = query.loader.as_deref()
					&& !crate::registry::compatibility::release_declares_loader(object, loader)
				{
					continue;
				}
				if let (Some(loader), Some(version)) = (query.loader.as_deref(), query.loader_version.as_deref()) {
					let ordering = match loader_scheme {
						Some(ordering) => ordering,
						None => {
							let resolved = crate::registry::compatibility::loader_ordering(&state, loader).await;
							loader_scheme = Some(resolved);
							resolved
						}
					};
					if !crate::registry::compatibility::release_matches_loader_version(object, loader, version, ordering) {
						continue;
					}
				}
				if let (Some(runtime), Some(version)) = (query.runtime.as_deref(), query.runtime_version.as_deref()) {
					let ordering = match runtime_scheme {
						Some(ordering) => ordering,
						None => {
							let resolved = crate::registry::compatibility::runtime_ordering(&state, runtime).await;
							runtime_scheme = Some(resolved);
							resolved
						}
					};
					if !crate::registry::compatibility::release_matches_runtime(object, version, ordering) {
						continue;
					}
				}
			}
			let (title, release) = match object.as_ref() {
				Some(object) => (describe_stored(object), crate::registry::views::summarize_release(object)),
				_ => (None, None),
			};
			entries.push(FeedEntryView {
				seq: row.seq,
				kind: row.kind.clone(),
				title,
				release,
				object: id_for(&row.object_digest),
				entry: id_for(&row.entry_digest),
				declared_at,
				previous: row.previous.as_deref().map(id_for),
			});
		}
		pages += 1;
		if (entries.len() as i64) >= limit
			|| fetched < limit
			|| pages >= state.capability.max_feed_scan_pages.max(1) as usize
		{
			break;
		}
	}
	let next = match entries.last() {
		Some(entry) => Some(entry.seq),
		None if scanned > query.after => Some(scanned),
		None => None,
	};
	let truncated = (entries.len() as i64) < limit
		&& scanned > query.after
		&& pages >= state.capability.max_feed_scan_pages.max(1) as usize;
	let page = FeedPage {
		project_id: project.id,
		head_seq: project.head_seq,
		entries,
		next,
		truncated,
	};
	Json(page).into_response()
}
