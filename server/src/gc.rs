use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use crate::routes::AppState;

#[derive(Debug, Clone, Copy, Default)]
pub struct Collected {
	pub staging: u64,
	pub blobs: u64,
}

pub async fn collect(
	state: &AppState,
	staging_retention_seconds: u64,
	blob_retention_seconds: u64,
) -> Result<Collected, String> {
	let staging_before = SystemTime::now() - Duration::from_secs(staging_retention_seconds);
	let staging = state
		.store
		.sweep_staging(staging_before)
		.await
		.map_err(|error| error.to_string())?;
	let referenced: HashSet<Vec<u8>> = state
		.metadata
		.referenced_blob_digests()
		.await
		.map_err(|error| error.to_string())?
		.into_iter()
		.collect();
	let cutoff = SystemTime::now() - Duration::from_secs(blob_retention_seconds);
	let mut blobs = 0;
	for (digest, modified) in state.store.list_committed().await.map_err(|error| error.to_string())? {
		if referenced.contains(digest.as_slice()) || modified >= cutoff {
			continue;
		}
		state
			.store
			.remove_committed(&digest)
			.await
			.map_err(|error| error.to_string())?;
		blobs += 1;
	}
	Ok(Collected { staging, blobs })
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use super::*;
	use crate::blob::BlobStore;
	use crate::store::MetadataStore;

	async fn state(directory: &std::path::Path) -> AppState {
		let store = Arc::new(BlobStore::new(directory).await.expect("blob store"));
		let metadata = Arc::new(
			MetadataStore::open(directory.join("metadata.sqlite"))
				.await
				.expect("metadata"),
		);
		let config = crate::config::Config {
			bind: "127.0.0.1:0".parse().expect("addr"),
			data_dir: directory.to_path_buf(),
			max_artifact_bytes: 1024,
			max_feed_page_entries: 100,
			max_response_bytes: 16_777_216,
			staging_retention_seconds: 3_600,
			blob_retention_seconds: 604_800,
			max_sync_pages: 200,
			requests_per_minute: 600,
			max_concurrent_syncs: 4,
			tls_extra_roots: None,
			max_feed_scan_pages: 50,
			skip_migrate_on_start: false,
			publishing: crate::config::Publishing::Review,
			allow_insecure_federation_local: false,
			web_dir: None,
		};
		AppState {
			store,
			metadata,
			capability: Arc::new(crate::capability::Capability::discover(&config)),
			login_limiter: Arc::new(crate::auth::LoginLimiter::new()),
			metrics: Arc::new(crate::metrics::Metrics::new()),
			rate_limiter: Arc::new(crate::ratelimit::RateLimiter::new()),
			web_dir: None,
		}
	}

	async fn commit_aged(state: &AppState, bytes: &[u8], modified: SystemTime) -> [u8; 32] {
		let staged = state.store.put_staged(bytes, 1024).await.expect("stage");
		let digest = state.store.commit(staged).await.expect("commit");
		std::fs::File::open(state.store.blob_path(&digest))
			.expect("open")
			.set_modified(modified)
			.expect("mtime");
		digest
	}

	#[tokio::test]
	async fn keeps_referenced_blobs_and_collects_orphans() {
		let directory = tempfile::tempdir().expect("tempdir");
		let state = state(directory.path()).await;
		let aged = SystemTime::now() - Duration::from_secs(86_400 + 60);
		let orphan = commit_aged(&state, b"orphan", aged).await;
		let referenced = commit_aged(&state, b"referenced", aged).await;
		state
			.metadata
			.index_artifact(&referenced, "project", &[9u8; 32])
			.await
			.expect("index");

		let collected = collect(&state, 3_600, 86_400).await.expect("collect");

		assert_eq!(collected.blobs, 1);
		assert!(state.store.size(&orphan).await.expect("size").is_none());
		assert!(state.store.size(&referenced).await.expect("size").is_some());
	}

	#[tokio::test]
	async fn leaves_fresh_blobs_alone() {
		let directory = tempfile::tempdir().expect("tempdir");
		let state = state(directory.path()).await;
		let fresh = commit_aged(&state, b"fresh", SystemTime::now()).await;

		let collected = collect(&state, 3_600, 86_400).await.expect("collect");

		assert_eq!(collected.blobs, 0);
		assert!(state.store.size(&fresh).await.expect("size").is_some());
	}
}
