use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

pub fn hash_password(password: &str) -> Result<String, PasswordError> {
	let salt = SaltString::encode_b64(&random_salt()).map_err(|_| PasswordError)?;
	Argon2::default()
		.hash_password(password.as_bytes(), &salt)
		.map(|hash| hash.to_string())
		.map_err(|_| PasswordError)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
	let Ok(parsed) = PasswordHash::new(hash) else {
		return false;
	};
	Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}

#[derive(Debug)]
pub struct PasswordError;

impl std::fmt::Display for PasswordError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("password hashing failed")
	}
}

impl std::error::Error for PasswordError {}

fn random_salt() -> [u8; 16] {
	let mut salt = [0u8; 16];
	if getrandom::fill(&mut salt).is_err() {
		panic!("operating system randomness is unavailable");
	}
	salt
}
