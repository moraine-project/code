use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

pub struct ArtifactFile {
	pub digest: [u8; 32],
	pub size: u64,
	pub filename: String,
	pub media_type: String,
}

pub fn describe(path: &Path) -> Result<ArtifactFile, String> {
	let mut file = std::fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
	let mut hasher = Sha256::new();
	let mut buffer = vec![0u8; 64 * 1024];
	let mut size = 0u64;
	loop {
		let read = file
			.read(&mut buffer)
			.map_err(|error| format!("{}: {error}", path.display()))?;
		if read == 0 {
			break;
		}
		hasher.update(&buffer[..read]);
		size += read as u64;
	}
	let filename = path
		.file_name()
		.and_then(|name| name.to_str())
		.ok_or_else(|| format!("{} has no file name", path.display()))?
		.to_string();
	Ok(ArtifactFile {
		digest: hasher.finalize().into(),
		size,
		filename: filename.clone(),
		media_type: media_type_for(&filename),
	})
}

pub fn media_type_for(filename: &str) -> String {
	let extension = filename.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase());
	match extension.as_deref() {
		Some("jar") => "application/java-archive",
		Some("zip") => "application/zip",
		Some("json") => "application/json",
		Some("toml") => "application/toml",
		_ => "application/octet-stream",
	}
	.to_string()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn knows_common_mod_media_types() {
		assert_eq!(media_type_for("mod.jar"), "application/java-archive");
		assert_eq!(media_type_for("pack.ZIP"), "application/zip");
		assert_eq!(media_type_for("LICENSE"), "application/octet-stream");
	}

	#[test]
	fn hashes_a_file_without_holding_it_in_memory() {
		let directory = tempfile::tempdir().expect("tempdir");
		let path = directory.path().join("mod.jar");
		std::fs::write(&path, b"hello world").expect("write");
		let described = describe(&path).expect("describe");
		assert_eq!(
			hex::encode(described.digest),
			"b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
		);
		assert_eq!(described.size, 11);
	}
}
