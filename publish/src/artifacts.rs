use std::path::Path;

use sha2::{Digest, Sha256};

pub struct ArtifactFile {
	pub digest: [u8; 32],
	pub size: u64,
	pub filename: String,
	pub media_type: String,
}

pub fn describe(path: &Path) -> Result<ArtifactFile, String> {
	let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
	let digest = Sha256::digest(&bytes).into();
	let filename = path
		.file_name()
		.and_then(|name| name.to_str())
		.ok_or_else(|| format!("{} has no file name", path.display()))?
		.to_string();
	let media_type = media_type_for(&filename);
	Ok(ArtifactFile {
		digest,
		size: bytes.len() as u64,
		filename,
		media_type,
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
}
