use std::path::Path;

use sha2::{Digest, Sha256};
use sqlx::sqlite::SqliteConnectOptions;

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
	sqlx::query(sqlx::AssertSqlSafe(format!("VACUUM INTO '{escaped}'")))
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

pub struct Verified {
	pub projects: i64,
	pub blobs: u64,
	pub bytes: u64,
}

pub async fn restore(config: &Config, directory: &Path, force: bool) -> Result<Verified, String> {
	let verified = verify(directory).await?;
	let metadata_path = config.data_dir.join("metadata.sqlite");
	let blobs_path = config.data_dir.join("blobs");
	if !force && (metadata_path.exists() || blobs_path.exists()) {
		return Err(format!(
			"{} already holds data; pass --force to replace it",
			config.data_dir.display()
		));
	}
	std::fs::create_dir_all(&config.data_dir).map_err(|error| error.to_string())?;
	std::fs::copy(directory.join("metadata.sqlite"), &metadata_path).map_err(|error| error.to_string())?;
	if blobs_path.exists() {
		std::fs::remove_dir_all(&blobs_path).map_err(|error| error.to_string())?;
	}
	std::fs::create_dir_all(&blobs_path).map_err(|error| error.to_string())?;
	for entry in std::fs::read_dir(directory.join("blobs")).map_err(|error| error.to_string())? {
		let entry = entry.map_err(|error| error.to_string())?;
		std::fs::copy(entry.path(), blobs_path.join(entry.file_name())).map_err(|error| error.to_string())?;
	}
	Ok(verified)
}

pub async fn verify(directory: &Path) -> Result<Verified, String> {
	let database = directory.join("metadata.sqlite");
	if !database.is_file() {
		return Err(format!("{} is missing", database.display()));
	}
	let options = SqliteConnectOptions::new()
		.filename(&database)
		.read_only(true)
		.create_if_missing(false);
	let pool = sqlx::SqlitePool::connect_with(options)
		.await
		.map_err(|error| error.to_string())?;
	let projects = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM projects")
		.fetch_one(&pool)
		.await
		.map_err(|error| error.to_string())?;
	let inventory = std::fs::read_to_string(directory.join("blobs.txt")).map_err(|error| error.to_string())?;
	let mut blobs = 0;
	let mut bytes = 0;
	for line in inventory.lines() {
		let Some((name, size)) = line.split_once(' ') else {
			return Err(format!("`{line}` is not an inventory entry"));
		};
		let expected: u64 = size.parse().map_err(|_| format!("`{size}` is not a size"))?;
		let path = directory.join("blobs").join(name);
		let contents = std::fs::read(&path).map_err(|error| format!("{name}: {error}"))?;
		if contents.len() as u64 != expected {
			return Err(format!("{name} is {} bytes, the inventory says {expected}", contents.len()));
		}
		if hex::encode(Sha256::digest(&contents)) != name {
			return Err(format!("{name} does not match its contents"));
		}
		blobs += 1;
		bytes += expected;
	}
	Ok(Verified { projects, blobs, bytes })
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
			max_concurrent_syncs: 4,
			tls_extra_roots: None,
			max_feed_scan_pages: 50,
			skip_migrate_on_start: false,
			publishing: crate::config::Publishing::Review,
			allow_insecure_federation_local: false,
			web_dir: None,
		}
	}

	#[tokio::test]
	async fn restores_a_published_project_into_a_fresh_directory() {
		let source = tempfile::tempdir().expect("source");
		let source_config = config(source.path()).await;
		let application = crate::test_support::app_in(source.path(), crate::config::Publishing::Open, false, 100, 600).await;
		let signer = crate::test_support::key(9);
		let (project_id, _) = crate::test_support::publish_project(&application, &signer).await;
		let out = tempfile::tempdir().expect("out");
		run(&source_config, out.path()).await.expect("backup");

		let target = tempfile::tempdir().expect("target");
		let target_config = config(target.path()).await;
		restore(&target_config, out.path(), false).await.expect("restore");

		let restored = crate::test_support::app_in(target.path(), crate::config::Publishing::Open, false, 100, 600).await;
		let request = axum::http::Request::get(format!("/v1/projects/{project_id}"))
			.body(axum::body::Body::empty())
			.expect("request");
		let response = tower::ServiceExt::oneshot(restored, request).await.expect("response");
		assert_eq!(response.status(), axum::http::StatusCode::OK);

		assert!(restore(&target_config, out.path(), false).await.is_err());
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
		assert_eq!(std::fs::read(&copied).expect("copy"), b"artifact");

		let verified = verify(out.path()).await.expect("verify");
		assert_eq!(verified.blobs, 1);
		assert_eq!(verified.bytes, 8);

		std::fs::write(&copied, b"tampered").expect("tamper");
		assert!(verify(out.path()).await.is_err());
	}
}
