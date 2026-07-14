pub mod catalog;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use moraine_install::{InstallError as PlanError, ModFile, Placement, safe_join};
use moraine_resolver::Lockfile;
use sha2::{Digest, Sha256};

pub trait BlobSource {
	fn fetch(&self, digest: &[u8; 32]) -> Result<Vec<u8>, String>;
}

#[derive(Debug)]
pub enum InstallError {
	Fetch {
		digest: String,
		detail: String,
	},
	MalformedDigest(String),
	SizeMismatch {
		digest: String,
		expected: u64,
		actual: u64,
	},
	DigestMismatch(String),
	Plan(PlanError),
	UnsafePath(String),
	Io(String),
}

impl std::fmt::Display for InstallError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Fetch { digest, detail } => write!(f, "could not fetch {digest}: {detail}"),
			Self::MalformedDigest(digest) => write!(f, "`{digest}` is not a sha256 digest"),
			Self::SizeMismatch {
				digest,
				expected,
				actual,
			} => {
				write!(f, "{digest} is {actual} bytes, expected {expected}")
			}
			Self::DigestMismatch(digest) => write!(f, "bytes do not match {digest}"),
			Self::Plan(error) => write!(f, "{error}"),
			Self::UnsafePath(path) => write!(f, "placement escapes the instance root: {path}"),
			Self::Io(detail) => write!(f, "filesystem error: {detail}"),
		}
	}
}

impl std::error::Error for InstallError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
	pub verified: usize,
	pub placements: Vec<Placement>,
}

#[derive(Debug)]
pub struct PreparedInstall {
	pub report: InstallReport,
	pub bytes: BTreeMap<[u8; 32], Vec<u8>>,
}

pub fn plan_install(lockfile: &Lockfile, blobs: &dyn BlobSource, adapter: &str) -> Result<PreparedInstall, InstallError> {
	let mut mods = Vec::with_capacity(lockfile.releases.len());
	let mut bytes_by_digest = BTreeMap::new();
	for release in &lockfile.releases {
		let digest = parse_digest(&release.artifact.digest)?;
		if bytes_by_digest.contains_key(&digest) {
			mods.push(ModFile {
				digest,
				filename: release.artifact.filename.clone(),
			});
			continue;
		}
		let bytes = blobs.fetch(&digest).map_err(|detail| InstallError::Fetch {
			digest: release.artifact.digest.clone(),
			detail,
		})?;
		if bytes.len() as u64 != release.artifact.size {
			return Err(InstallError::SizeMismatch {
				digest: release.artifact.digest.clone(),
				expected: release.artifact.size,
				actual: bytes.len() as u64,
			});
		}
		if Sha256::digest(&bytes).as_slice() != digest {
			return Err(InstallError::DigestMismatch(release.artifact.digest.clone()));
		}
		bytes_by_digest.insert(digest, bytes);
		mods.push(ModFile {
			digest,
			filename: release.artifact.filename.clone(),
		});
	}
	let plan = moraine_install::plan(adapter, &mods, &[]).map_err(InstallError::Plan)?;
	let report = InstallReport {
		verified: bytes_by_digest.len(),
		placements: plan.placements,
	};
	Ok(PreparedInstall {
		report,
		bytes: bytes_by_digest,
	})
}

pub fn apply_install(prepared: &PreparedInstall, instance_root: &Path) -> Result<Vec<PathBuf>, InstallError> {
	let mut written = Vec::with_capacity(prepared.report.placements.len());
	for placement in &prepared.report.placements {
		let destination = safe_join(instance_root, &placement.relative_path)
			.ok_or_else(|| InstallError::UnsafePath(placement.relative_path.display().to_string()))?;
		let bytes = prepared.bytes.get(&placement.digest).ok_or_else(|| InstallError::Fetch {
			digest: hex::encode(placement.digest),
			detail: "not verified".to_string(),
		})?;
		if let Some(parent) = destination.parent() {
			std::fs::create_dir_all(parent).map_err(|error| InstallError::Io(error.to_string()))?;
		}
		let staging = destination.with_extension("part");
		std::fs::write(&staging, bytes).map_err(|error| InstallError::Io(error.to_string()))?;
		std::fs::rename(&staging, &destination).map_err(|error| InstallError::Io(error.to_string()))?;
		written.push(destination);
	}
	Ok(written)
}

pub struct FilesystemBlobs {
	root: PathBuf,
}

impl FilesystemBlobs {
	pub fn new(root: impl Into<PathBuf>) -> Self {
		Self { root: root.into() }
	}
}

impl BlobSource for FilesystemBlobs {
	fn fetch(&self, digest: &[u8; 32]) -> Result<Vec<u8>, String> {
		std::fs::read(self.root.join(hex::encode(digest))).map_err(|error| error.to_string())
	}
}

pub struct HttpBlobs {
	client: reqwest::blocking::Client,
	base: String,
}

impl HttpBlobs {
	pub fn new(base: &str, allow_http_local: bool) -> Result<Self, String> {
		validate_url(base, allow_http_local)?;
		let client = reqwest::blocking::Client::builder()
			.timeout(std::time::Duration::from_secs(30))
			.redirect(reqwest::redirect::Policy::none())
			.build()
			.map_err(|error| error.to_string())?;
		Ok(Self {
			client,
			base: base.trim_end_matches('/').to_string(),
		})
	}
}

impl BlobSource for HttpBlobs {
	fn fetch(&self, digest: &[u8; 32]) -> Result<Vec<u8>, String> {
		let response = self
			.client
			.get(format!("{}/v1/blobs/sha256/{}", self.base, hex::encode(digest)))
			.send()
			.map_err(|error| error.to_string())?;
		if !response.status().is_success() {
			return Err(format!("home returned {}", response.status()));
		}
		response
			.bytes()
			.map(|bytes| bytes.to_vec())
			.map_err(|error| error.to_string())
	}
}

fn validate_url(value: &str, allow_http_local: bool) -> Result<(), String> {
	let url = reqwest::Url::parse(value).map_err(|error| error.to_string())?;
	match url.scheme() {
		"https" => Ok(()),
		"http" => {
			let host = url.host_str().unwrap_or_default().to_string();
			let loopback = host == "localhost"
				|| host
					.parse::<std::net::IpAddr>()
					.map(|address| address.is_loopback())
					.unwrap_or(false);
			if allow_http_local && loopback {
				Ok(())
			} else {
				Err("http is only allowed for loopback when explicitly enabled".to_string())
			}
		}
		_ => Err("home url must use https".to_string()),
	}
}

fn parse_digest(value: &str) -> Result<[u8; 32], InstallError> {
	let hex = value.strip_prefix("sha256:").unwrap_or(value);
	hex::decode(hex)
		.map_err(|_| InstallError::MalformedDigest(value.to_string()))?
		.try_into()
		.map_err(|_| InstallError::MalformedDigest(value.to_string()))
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;
	use std::io::{Read, Write};
	use std::net::TcpListener;

	use moraine_resolver::{LockedArtifact, LockedRelease};

	use super::*;

	#[derive(Default)]
	struct MemoryBlobs {
		entries: BTreeMap<[u8; 32], Vec<u8>>,
	}

	impl BlobSource for MemoryBlobs {
		fn fetch(&self, digest: &[u8; 32]) -> Result<Vec<u8>, String> {
			self.entries.get(digest).cloned().ok_or_else(|| "missing".to_string())
		}
	}

	fn lockfile(digest: [u8; 32], size: u64) -> Lockfile {
		Lockfile {
			lockfile_version: 1,
			trust_policy_version: 1,
			game_id: "gd:sha256:game".to_string(),
			game_version: "1.20.1".to_string(),
			loader_id: None,
			loader_version: None,
			runtime_id: None,
			runtime_version: None,
			side: "client".to_string(),
			releases: vec![LockedRelease {
				project_id: "gd:sha256:project".to_string(),
				release_id: "gd:sha256:release".to_string(),
				human_version: "1.0.0".to_string(),
				artifact: LockedArtifact {
					digest: format!("sha256:{}", hex::encode(digest)),
					size,
					filename: "example.jar".to_string(),
				},
				dependencies: Vec::new(),
			}],
		}
	}

	#[test]
	fn verifies_then_places_a_locked_artifact() {
		let bytes = b"mod bytes".to_vec();
		let digest: [u8; 32] = Sha256::digest(&bytes).into();
		let mut blobs = MemoryBlobs::default();
		blobs.entries.insert(digest, bytes);
		let directory = tempfile::tempdir().expect("tempdir");

		let prepared = plan_install(&lockfile(digest, 9), &blobs, "minecraft/default").expect("plan");
		assert_eq!(prepared.report.verified, 1);
		let written = apply_install(&prepared, directory.path()).expect("apply");
		assert_eq!(written, vec![directory.path().join("mods/example.jar")]);
		assert!(written[0].exists());
	}

	#[test]
	fn refuses_bytes_that_do_not_match_the_digest() {
		let digest: [u8; 32] = Sha256::digest(b"real").into();
		let mut blobs = MemoryBlobs::default();
		blobs.entries.insert(digest, b"tampered".to_vec());
		let error = plan_install(&lockfile(digest, 8), &blobs, "minecraft/default").expect_err("reject");
		assert!(matches!(error, InstallError::DigestMismatch(_)));
	}

	#[test]
	fn fetches_a_blob_over_loopback_http() {
		let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
		let address = listener.local_addr().expect("addr");
		let body = b"blob bytes".to_vec();
		let served = body.clone();
		let handle = std::thread::spawn(move || {
			let (mut socket, _) = listener.accept().expect("accept");
			let mut request = [0u8; 1024];
			let _ = socket.read(&mut request);
			let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", served.len());
			let _ = socket.write_all(header.as_bytes());
			let _ = socket.write_all(&served);
		});

		let blobs = HttpBlobs::new(&format!("http://127.0.0.1:{}", address.port()), true).expect("client");
		let digest: [u8; 32] = Sha256::digest(&body).into();
		assert_eq!(blobs.fetch(&digest).expect("fetch"), body);
		handle.join().expect("join");

		assert!(HttpBlobs::new("http://example.org", false).is_err());
	}
}
