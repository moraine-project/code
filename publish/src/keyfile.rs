use std::path::Path;

use moraine_crypto::SigningKey;

pub fn load(path: &Path) -> Result<SigningKey, String> {
	let text = std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
	let bytes = hex::decode(text.trim()).map_err(|_| format!("{} is not a hex key", path.display()))?;
	let seed: [u8; 32] = bytes
		.try_into()
		.map_err(|_| format!("{} must hold 32 bytes", path.display()))?;
	Ok(SigningKey::from_seed(&seed))
}

pub fn create(path: &Path) -> Result<SigningKey, String> {
	if path.exists() {
		return Err(format!("{} already exists", path.display()));
	}
	let mut seed = [0u8; 32];
	getrandom::fill(&mut seed).map_err(|_| "operating system randomness is unavailable".to_string())?;
	std::fs::write(path, format!("{}\n", hex::encode(seed))).map_err(|error| format!("{}: {error}", path.display()))?;
	restrict_permissions(path)?;
	Ok(SigningKey::from_seed(&seed))
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), String> {
	use std::os::unix::fs::PermissionsExt;
	std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
		.map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), String> {
	Ok(())
}
