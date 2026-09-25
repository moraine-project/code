use std::io::{Read, Write};
use std::path::Path;

use moraine_crypto::SigningKey;
use zeroize::Zeroizing;

pub fn load(path: &Path) -> Result<SigningKey, String> {
	let file = std::fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		let mode = file
			.metadata()
			.map_err(|error| format!("{}: {error}", path.display()))?
			.permissions()
			.mode();
		if mode & 0o077 != 0 {
			return Err(format!(
				"{} is readable by others (mode {:o}); run `chmod 600 {}` before using it",
				path.display(),
				mode & 0o777,
				path.display()
			));
		}
	}
	let mut encoded = Zeroizing::new(Vec::new());
	file.take(4097)
		.read_to_end(&mut encoded)
		.map_err(|error| format!("{}: {error}", path.display()))?;
	if encoded.len() > 4096 {
		return Err(format!("{} is larger than a key file", path.display()));
	}
	let text = std::str::from_utf8(&encoded).map_err(|_| format!("{} is not a hex key", path.display()))?;
	let bytes = Zeroizing::new(hex::decode(text.trim()).map_err(|_| format!("{} is not a hex key", path.display()))?);
	if bytes.len() != 32 {
		return Err(format!("{} must hold 32 bytes", path.display()));
	}
	let mut seed = Zeroizing::new([0u8; 32]);
	seed.copy_from_slice(&bytes);
	Ok(SigningKey::from_seed(&seed))
}

pub fn create(path: &Path) -> Result<SigningKey, String> {
	let mut seed = Zeroizing::new([0u8; 32]);
	getrandom::fill(&mut *seed).map_err(|_| "operating system randomness is unavailable".to_string())?;
	let mut options = std::fs::OpenOptions::new();
	options.write(true).create_new(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		options.mode(0o600);
	}
	let mut file = options.open(path).map_err(|error| format!("{}: {error}", path.display()))?;
	file.write_all(format!("{}\n", hex::encode(*seed)).as_bytes())
		.map_err(|error| format!("{}: {error}", path.display()))?;
	Ok(SigningKey::from_seed(&seed))
}

#[cfg(all(test, unix))]
mod tests {
	use std::os::unix::fs::PermissionsExt;

	use super::*;

	#[test]
	fn rejects_a_key_file_readable_by_other_users() {
		let directory = tempfile::tempdir().expect("tempdir");
		let path = directory.path().join("publisher.key");
		create(&path).expect("key");
		std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("permissions");
		let error = load(&path).expect_err("insecure key");
		assert!(error.contains("readable by others"), "{error}");
		std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("permissions");
		load(&path).expect("private key");
	}
}
