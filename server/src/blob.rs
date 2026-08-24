use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fmt, io};

use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt};
use object_store::path::Path as ObjectPath;
use object_store::{GetOptions, GetRange, ObjectStore, ObjectStoreExt};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

pub type BlobStream = futures_util::stream::BoxStream<'static, Result<Bytes, io::Error>>;

pub struct BlobStore {
	staging: PathBuf,
	committed: Committed,
}

enum Committed {
	Local {
		root: PathBuf,
	},
	Object {
		store: Arc<dyn ObjectStore>,
		prefix: String,
	},
}

#[derive(Debug)]
pub struct StagedBlob {
	path: PathBuf,
	pub digest: [u8; 32],
	pub size: u64,
}

#[derive(Debug)]
pub enum BlobError {
	Io(io::Error),
	TooLarge {
		limit: u64,
	},
}

impl fmt::Display for BlobError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Io(error) => write!(f, "blob storage error: {error}"),
			Self::TooLarge { limit } => write!(f, "blob exceeds the {limit} byte limit"),
		}
	}
}

impl std::error::Error for BlobError {}

impl From<io::Error> for BlobError {
	fn from(error: io::Error) -> Self {
		Self::Io(error)
	}
}

impl BlobStore {
	pub async fn new(root: impl AsRef<Path>) -> io::Result<Self> {
		let root = root.as_ref();
		let staging = root.join("staging");
		let blobs = root.join("blobs");
		tokio::fs::create_dir_all(&staging).await?;
		tokio::fs::create_dir_all(&blobs).await?;
		Ok(Self {
			staging,
			committed: Committed::Local { root: blobs },
		})
	}

	pub async fn with_object_store(
		store: Arc<dyn ObjectStore>,
		prefix: impl Into<String>,
		staging: impl AsRef<Path>,
	) -> io::Result<Self> {
		let staging = staging.as_ref().to_path_buf();
		tokio::fs::create_dir_all(&staging).await?;
		Ok(Self {
			staging,
			committed: Committed::Object {
				store,
				prefix: prefix.into(),
			},
		})
	}

	#[cfg(test)]
	pub fn local_path(&self, digest: &[u8; 32]) -> Option<PathBuf> {
		match &self.committed {
			Committed::Local { root } => Some(root.join(hex::encode(digest))),
			Committed::Object { .. } => None,
		}
	}

	pub async fn put_staged<R>(&self, mut reader: R, max_bytes: u64) -> Result<StagedBlob, BlobError>
	where
		R: AsyncRead + Unpin,
	{
		let path = self.staging.join(staging_name());
		let mut file = tokio::fs::File::create(&path).await?;
		let mut hasher = Sha256::new();
		let mut buffer = vec![0u8; 64 * 1024];
		let mut size = 0u64;
		loop {
			let read = reader.read(&mut buffer).await?;
			if read == 0 {
				break;
			}
			size += read as u64;
			if size > max_bytes {
				drop(file);
				let _ = tokio::fs::remove_file(&path).await;
				return Err(BlobError::TooLarge { limit: max_bytes });
			}
			hasher.update(&buffer[..read]);
			file.write_all(&buffer[..read]).await?;
		}
		file.flush().await?;
		file.sync_all().await?;
		drop(file);
		let digest = hasher.finalize().into();
		Ok(StagedBlob { path, digest, size })
	}

	pub async fn commit(&self, staged: StagedBlob) -> io::Result<[u8; 32]> {
		match &self.committed {
			Committed::Local { root } => {
				let destination = root.join(hex::encode(staged.digest));
				match tokio::fs::rename(&staged.path, &destination).await {
					Ok(()) => Ok(staged.digest),
					Err(error) => {
						if destination.exists() {
							let _ = tokio::fs::remove_file(&staged.path).await;
							Ok(staged.digest)
						} else {
							Err(error)
						}
					}
				}
			}
			Committed::Object { .. } => {
				let location = self.object_path(&staged.digest);
				let result = self.upload(&location, &staged.path).await;
				let _ = tokio::fs::remove_file(&staged.path).await;
				result?;
				Ok(staged.digest)
			}
		}
	}

	pub async fn size(&self, digest: &[u8; 32]) -> io::Result<Option<u64>> {
		match &self.committed {
			Committed::Local { root } => match tokio::fs::metadata(root.join(hex::encode(digest))).await {
				Ok(metadata) => Ok(Some(metadata.len())),
				Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
				Err(error) => Err(error),
			},
			Committed::Object { store, .. } => match store.head(&self.object_path(digest)).await {
				Ok(meta) => Ok(Some(meta.size)),
				Err(object_store::Error::NotFound { .. }) => Ok(None),
				Err(error) => Err(io::Error::other(error.to_string())),
			},
		}
	}

	pub async fn read(&self, digest: &[u8; 32], range: Option<(u64, u64)>) -> io::Result<Option<BlobStream>> {
		match &self.committed {
			Committed::Local { root } => {
				let mut file = match tokio::fs::File::open(root.join(hex::encode(digest))).await {
					Ok(file) => file,
					Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
					Err(error) => return Err(error),
				};
				let stream = match range {
					Some((start, end)) => {
						file.seek(std::io::SeekFrom::Start(start)).await?;
						let length = end.saturating_sub(start) + 1;
						either_stream(file.take(length))
					}
					None => either_stream(file),
				};
				Ok(Some(stream))
			}
			Committed::Object { store, .. } => {
				let options = match range {
					Some((start, end)) => GetOptions {
						range: Some(GetRange::Bounded(start..end.saturating_add(1))),
						..Default::default()
					},
					None => GetOptions::default(),
				};
				match store.get_opts(&self.object_path(digest), options).await {
					Ok(result) => Ok(Some(result.into_stream().map_err(io::Error::other).boxed())),
					Err(object_store::Error::NotFound { .. }) => Ok(None),
					Err(error) => Err(io::Error::other(error.to_string())),
				}
			}
		}
	}

	pub async fn list_committed(&self) -> io::Result<Vec<([u8; 32], std::time::SystemTime)>> {
		match &self.committed {
			Committed::Local { root } => {
				let mut entries = tokio::fs::read_dir(root).await?;
				let mut blobs = Vec::new();
				while let Some(entry) = entries.next_entry().await? {
					let Ok(name) = entry.file_name().into_string() else {
						continue;
					};
					let Some(digest) = digest_from_hex(&name) else {
						continue;
					};
					blobs.push((digest, entry.metadata().await?.modified()?));
				}
				Ok(blobs)
			}
			Committed::Object { store, prefix } => {
				let root = ObjectPath::from(format!("{prefix}/blobs"));
				let mut listing = store.list(Some(&root));
				let mut blobs = Vec::new();
				while let Some(item) = listing.next().await {
					let meta = item.map_err(|error| io::Error::other(error.to_string()))?;
					let Some(name) = meta.location.filename() else {
						continue;
					};
					let Some(digest) = digest_from_hex(name) else {
						continue;
					};
					let seconds = u64::try_from(meta.last_modified.timestamp()).unwrap_or(0);
					blobs.push((digest, std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds)));
				}
				Ok(blobs)
			}
		}
	}

	pub async fn remove_committed(&self, digest: &[u8; 32]) -> io::Result<()> {
		match &self.committed {
			Committed::Local { root } => match tokio::fs::remove_file(root.join(hex::encode(digest))).await {
				Ok(()) => Ok(()),
				Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
				Err(error) => Err(error),
			},
			Committed::Object { store, .. } => match store.delete(&self.object_path(digest)).await {
				Ok(()) => Ok(()),
				Err(object_store::Error::NotFound { .. }) => Ok(()),
				Err(error) => Err(io::Error::other(error.to_string())),
			},
		}
	}

	pub async fn sweep_staging(&self, before: std::time::SystemTime) -> io::Result<u64> {
		let mut entries = tokio::fs::read_dir(&self.staging).await?;
		let mut removed = 0;
		while let Some(entry) = entries.next_entry().await? {
			if entry.file_name().to_string_lossy().ends_with(".part")
				&& entry.metadata().await?.modified()? < before
				&& tokio::fs::remove_file(entry.path()).await.is_ok()
			{
				removed += 1;
			}
		}
		Ok(removed)
	}

	fn object_path(&self, digest: &[u8; 32]) -> ObjectPath {
		let Committed::Object { prefix, .. } = &self.committed else {
			panic!("object path requested from a local store");
		};
		ObjectPath::from(format!("{prefix}/blobs/{}", hex::encode(digest)))
	}

	async fn upload(&self, location: &ObjectPath, file: &Path) -> io::Result<()> {
		let Committed::Object { store, .. } = &self.committed else {
			panic!("upload requested from a local store");
		};
		let mut upload = object_store::WriteMultipart::new(store.put_multipart(location).await.map_err(object_error)?);
		let mut file = tokio::fs::File::open(file).await?;
		let mut buffer = vec![0u8; 1024 * 1024];
		loop {
			let read = file.read(&mut buffer).await?;
			if read == 0 {
				break;
			}
			upload.write(&buffer[..read]);
			upload.wait_for_capacity(4).await.map_err(object_error)?;
		}
		upload.finish().await.map_err(object_error)?;
		Ok(())
	}
}

pub async fn open_store(config: &crate::config::Config) -> io::Result<BlobStore> {
	let Some(bucket) = config.s3.bucket.as_deref().filter(|bucket| !bucket.trim().is_empty()) else {
		return BlobStore::new(&config.data_dir).await;
	};
	let store = s3_object_store(bucket, &config.s3)?;
	BlobStore::with_object_store(store, config.s3.prefix.clone(), config.data_dir.join("staging")).await
}

pub(crate) fn s3_object_store(bucket: &str, settings: &crate::config::S3Settings) -> io::Result<Arc<dyn ObjectStore>> {
	use object_store::aws::AmazonS3Builder;

	let mut builder = AmazonS3Builder::new()
		.with_bucket_name(bucket)
		.with_virtual_hosted_style_request(false);
	if let Some(endpoint) = settings.endpoint.as_deref().filter(|value| !value.trim().is_empty()) {
		if endpoint.starts_with("http://") {
			builder = builder.with_allow_http(true);
		}
		builder = builder.with_endpoint(endpoint);
	}
	if let Some(region) = settings.region.as_deref().filter(|value| !value.trim().is_empty()) {
		builder = builder.with_region(region);
	}
	if let (Some(key), Some(secret)) = (
		settings.access_key_id.as_deref().filter(|value| !value.trim().is_empty()),
		settings.secret_access_key.as_deref().filter(|value| !value.trim().is_empty()),
	) {
		builder = builder.with_access_key_id(key).with_secret_access_key(secret);
	}
	Ok(Arc::new(builder.build().map_err(io::Error::other)?))
}

fn either_stream<R>(reader: R) -> BlobStream
where
	R: tokio::io::AsyncRead + Send + 'static,
{
	tokio_util::io::ReaderStream::new(reader).boxed()
}

fn object_error(error: object_store::Error) -> io::Error {
	io::Error::other(error.to_string())
}

fn digest_from_hex(name: &str) -> Option<[u8; 32]> {
	if name.len() != 64 {
		return None;
	}
	<[u8; 32]>::try_from(hex::decode(name).ok()?.as_slice()).ok()
}

fn staging_name() -> String {
	let counter = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
	let nanos = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|d| d.as_nanos())
		.unwrap_or(0);
	format!("{nanos:x}-{counter:x}.part")
}

#[cfg(test)]
mod tests {
	use object_store::memory::InMemory;

	use super::*;

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
