#[cfg(test)]
mod tests {
	use std::path::Path;
	use std::sync::Arc;

	use futures_util::StreamExt;
	use object_store::memory::InMemory;

	use crate::blob::{BlobError, BlobStore, s3_object_store};

	#[tokio::test]
	async fn commit_is_content_addressed_and_idempotent() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = BlobStore::new(directory.path()).await.expect("store");
		let staged = store.put_staged(b"hello world".as_slice(), 1024).await.expect("stage");
		let digest = staged.digest;
		assert_eq!(
			hex::encode(digest),
			"b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
		);
		store.commit(staged).await.expect("commit");
		let staged = store.put_staged(b"hello world".as_slice(), 1024).await.expect("restage");
		assert_eq!(digest, staged.digest);
		store.commit(staged).await.expect("commit again");

		let mut stream = store.read(&digest, None).await.expect("read").expect("present");
		let mut contents = Vec::new();
		while let Some(chunk) = stream.next().await {
			contents.extend_from_slice(&chunk.expect("chunk"));
		}
		assert_eq!(contents, b"hello world");
	}

	#[tokio::test]
	async fn sweeps_abandoned_staging_files() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = BlobStore::new(directory.path()).await.expect("store");
		let abandoned = directory.path().join("staging").join("abandoned.part");
		std::fs::write(&abandoned, b"partial").expect("write");
		let aged = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
		std::fs::File::open(&abandoned)
			.expect("open")
			.set_modified(aged)
			.expect("mtime");
		let fresh = directory.path().join("staging").join("fresh.part");
		std::fs::write(&fresh, b"partial").expect("write");

		let removed = store
			.sweep_staging(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
			.await;

		assert_eq!(removed.expect("sweep"), 1);
		assert!(!abandoned.exists());
		assert!(fresh.exists());
	}

	#[tokio::test]
	async fn oversized_uploads_are_rejected_and_cleaned_up() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = BlobStore::new(directory.path()).await.expect("store");
		let error = store.put_staged(b"0123456789".as_slice(), 4).await.expect_err("too large");
		assert!(matches!(error, BlobError::TooLarge { limit: 4 }));
		let mut entries = tokio::fs::read_dir(directory.path().join("staging")).await.expect("read dir");
		assert!(entries.next_entry().await.expect("entry").is_none());
	}

	async fn object_store(directory: &Path) -> BlobStore {
		BlobStore::with_object_store(Arc::new(InMemory::new()), "moraine", directory.join("staging"))
			.await
			.expect("object store")
	}

	#[tokio::test]
	async fn a_live_s3_store_round_trips() {
		let (Ok(endpoint), Ok(bucket), Ok(region)) = (
			std::env::var("MORAINE_TEST_S3_ENDPOINT"),
			std::env::var("MORAINE_TEST_S3_BUCKET"),
			std::env::var("MORAINE_TEST_S3_REGION"),
		) else {
			return;
		};
		let directory = tempfile::tempdir().expect("tempdir");
		let settings = crate::config::S3Settings {
			bucket: Some(bucket.clone()),
			endpoint: Some(endpoint),
			region: Some(region),
			access_key_id: std::env::var("MORAINE_TEST_S3_ACCESS_KEY_ID").ok(),
			secret_access_key: std::env::var("MORAINE_TEST_S3_SECRET_ACCESS_KEY").ok(),
			prefix: format!(
				"moraine-test-{}",
				std::time::SystemTime::now()
					.duration_since(std::time::UNIX_EPOCH)
					.map(|duration| duration.as_nanos())
					.unwrap_or(0)
			),
		};
		let store = BlobStore::with_object_store(
			s3_object_store(&bucket, &settings).expect("s3"),
			settings.prefix.clone(),
			directory.path().join("staging"),
		)
		.await
		.expect("store");

		let staged = store.put_staged(b"live bytes".as_slice(), 1024).await.expect("stage");
		let digest = store.commit(staged).await.expect("commit");
		assert_eq!(store.size(&digest).await.expect("size"), Some(10));
		assert_eq!(store.list_committed().await.expect("list").len(), 1);

		let mut stream = store.read(&digest, Some((5, 9))).await.expect("read").expect("present");
		let mut contents = Vec::new();
		while let Some(chunk) = stream.next().await {
			contents.extend_from_slice(&chunk.expect("chunk"));
		}
		assert_eq!(contents, b"bytes");

		store.remove_committed(&digest).await.expect("remove");
		assert_eq!(store.list_committed().await.expect("list").len(), 0);
	}

	#[tokio::test]
	async fn an_object_backed_store_commits_serves_ranges_and_deletes() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = object_store(directory.path()).await;
		let bytes = b"0123456789".to_vec();
		let staged = store.put_staged(bytes.as_slice(), 1024).await.expect("stage");
		let digest = store.commit(staged).await.expect("commit");

		assert_eq!(store.size(&digest).await.expect("size"), Some(10));
		assert_eq!(store.list_committed().await.expect("list").len(), 1);

		let mut whole = store.read(&digest, None).await.expect("read").expect("present");
		let mut contents = Vec::new();
		while let Some(chunk) = whole.next().await {
			contents.extend_from_slice(&chunk.expect("chunk"));
		}
		assert_eq!(contents, bytes);

		let mut ranged = store.read(&digest, Some((2, 5))).await.expect("read").expect("present");
		let mut contents = Vec::new();
		while let Some(chunk) = ranged.next().await {
			contents.extend_from_slice(&chunk.expect("chunk"));
		}
		assert_eq!(contents, b"2345");

		let staged = store.put_staged(b"unused".as_slice(), 1024).await.expect("stage");
		let missing = staged.digest;
		let _ = staged;
		assert_eq!(store.size(&missing).await.expect("size"), None);

		store.remove_committed(&digest).await.expect("remove");
		assert_eq!(store.list_committed().await.expect("list").len(), 0);
	}
}
