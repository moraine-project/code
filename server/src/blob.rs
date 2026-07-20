use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fmt, io};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct BlobStore {
	blobs: PathBuf,
	staging: PathBuf,
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
		let blobs = root.join("blobs");
		let staging = root.join("staging");
		tokio::fs::create_dir_all(&blobs).await?;
		tokio::fs::create_dir_all(&staging).await?;
		Ok(Self { blobs, staging })
	}

	pub fn blob_path(&self, digest: &[u8; 32]) -> PathBuf {
		self.blobs.join(hex::encode(digest))
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
		let destination = self.blob_path(&staged.digest);
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

	pub async fn open(&self, digest: &[u8; 32]) -> io::Result<Option<tokio::fs::File>> {
		match tokio::fs::File::open(self.blob_path(digest)).await {
			Ok(file) => Ok(Some(file)),
			Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
			Err(error) => Err(error),
		}
	}

	pub async fn list_committed(&self) -> io::Result<Vec<([u8; 32], std::time::SystemTime)>> {
		let mut entries = tokio::fs::read_dir(&self.blobs).await?;
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

	pub async fn remove_committed(&self, digest: &[u8; 32]) -> io::Result<()> {
		match tokio::fs::remove_file(self.blob_path(digest)).await {
			Ok(()) => Ok(()),
			Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
			Err(error) => Err(error),
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

	pub async fn size(&self, digest: &[u8; 32]) -> io::Result<Option<u64>> {
		match tokio::fs::metadata(self.blob_path(digest)).await {
			Ok(metadata) => Ok(Some(metadata.len())),
			Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
			Err(error) => Err(error),
		}
	}
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

		let mut file = store.open(&digest).await.expect("open").expect("present");
		let mut contents = Vec::new();
		file.read_to_end(&mut contents).await.expect("read");
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
}
