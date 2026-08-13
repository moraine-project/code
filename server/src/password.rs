use argon2::Argon2;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};

pub fn hash_password(password: &str) -> Result<String, PasswordError> {
	Argon2::default()
		.hash_password(password.as_bytes())
		.map(|hash| hash.to_string())
		.map_err(|_| PasswordError)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
	Argon2::default().verify_password(password.as_bytes(), hash).is_ok()
}

#[derive(Debug)]
pub struct PasswordError;

impl std::fmt::Display for PasswordError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("password hashing failed")
	}
}

impl std::error::Error for PasswordError {}
