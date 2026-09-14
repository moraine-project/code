use std::io::Write;
use std::path::Path;

use moraine_crypto::SigningKey;
use zeroize::Zeroizing;

pub fn load(path: &Path) -> Result<SigningKey, String> {
	let text = std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
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
