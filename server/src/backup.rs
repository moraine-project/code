use std::path::Path;

use crate::blob::BlobStore;
use crate::config::Config;
use crate::store::MetadataStore;

pub struct Summary {
	pub projects: i64,
	pub blobs: u64,
	pub bytes: u64,
}

pub async fn run(config: &Config, out: &Path) -> Result<Summary, String> {
	std::fs::create_dir_all(out).map_err(|error| error.to_string())?;
	let metadata = MetadataStore::open(config.data_dir.join("metadata.sqlite"))
		.await
		.map_err(|error| error.to_string())?;
	let database = out.join("metadata.sqlite");
	let _ = std::fs::remove_file(&database);
	let escaped = database.to_string_lossy().replace('\'', "''");
	sqlx::query(&format!("VACUUM INTO '{escaped}'"))
		.execute(&metadata.pool)
		.await
		.map_err(|error| error.to_string())?;

	let store = BlobStore::new(&config.data_dir).await.map_err(|error| error.to_string())?;
	let copied = out.join("blobs");
	std::fs::create_dir_all(&copied).map_err(|error| error.to_string())?;
	let mut inventory = String::new();
	let mut blobs = 0;
	let mut bytes = 0;
	for (digest, _) in store.list_committed().await.map_err(|error| error.to_string())? {
		let name = hex::encode(digest);
		let size = store.size(&digest).await.map_err(|error| error.to_string())?.unwrap_or(0);
		std::fs::copy(store.blob_path(&digest), copied.join(&name)).map_err(|error| error.to_string())?;
		inventory.push_str(&format!("{name} {size}\n"));
		blobs += 1;
		bytes += size;
	}
	std::fs::write(out.join("blobs.txt"), inventory).map_err(|error| error.to_string())?;
	let projects = metadata.metrics_snapshot().await.map_err(|error| error.to_string())?.projects;
	Ok(Summary { projects, blobs, bytes })
}

#[cfg(test)]
mod tests {
	use super::*;

	async fn config(directory: &Path) -> Config {
		Config {
			bind: "127.0.0.1:0".parse().expect("addr"),
			data_dir: directory.to_path_buf(),
			max_artifact_bytes: 1024,
			max_feed_page_entries: 100,
			max_response_bytes: 16_777_216,
			staging_retention_seconds: 3_600,
			blob_retention_seconds: 604_800,
			max_sync_pages: 200,
			requests_per_minute: 600,
			publishing: crate::config::Publishing::Review,
			allow_insecure_federation_local: false,
			web_dir: None,
		}
	}

	#[tokio::test]
	async fn records_a_snapshot_inventory_and_bytes() {
		let data = tempfile::tempdir().expect("data");
		let config = config(data.path()).await;
		let store = BlobStore::new(&config.data_dir).await.expect("blob store");
		let staged = store.put_staged(b"artifact".as_slice(), 1024).await.expect("stage");
		let digest = store.commit(staged).await.expect("commit");
		MetadataStore::open(config.data_dir.join("metadata.sqlite"))
			.await
			.expect("metadata");
		let out = tempfile::tempdir().expect("out");

		let summary = run(&config, out.path()).await.expect("backup");

		assert_eq!(summary.blobs, 1);
		assert_eq!(summary.bytes, 8);
		assert!(out.path().join("metadata.sqlite").is_file());
		let inventory = std::fs::read_to_string(out.path().join("blobs.txt")).expect("inventory");
		assert_eq!(inventory, format!("{} 8\n", hex::encode(digest)));
		let copied = out.path().join("blobs").join(hex::encode(digest));
		assert_eq!(std::fs::read(copied).expect("copy"), b"artifact");
	}
}
