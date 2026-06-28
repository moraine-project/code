use std::fmt;

use ed25519_dalek::{Signer, Verifier};
use sha2::{Digest, Sha256};

use crate::CryptoError;

pub const ALG_ED25519: u8 = 0x01;

pub fn alg_label(alg: u8) -> Option<&'static str> {
	match alg {
		ALG_ED25519 => Some("ed25519"),
		_ => None,
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyId {
	alg: u8,
	digest: [u8; 32],
}

impl KeyId {
	pub fn from_digest(alg: u8, digest: [u8; 32]) -> Self {
		Self { alg, digest }
	}

	pub const fn algorithm(self) -> u8 {
		self.alg
	}

	pub const fn digest(self) -> [u8; 32] {
		self.digest
	}

	pub fn parse(text: &str) -> Result<Self, CryptoError> {
		let (label, hex_digest) = text.split_once(':').ok_or(CryptoError::MalformedKeyId)?;
		let alg = match label {
			"ed25519" => ALG_ED25519,
			_ => return Err(CryptoError::MalformedKeyId),
		};
		let bytes = hex::decode(hex_digest).map_err(|_| CryptoError::MalformedKeyId)?;
		let digest: [u8; 32] = bytes.try_into().map_err(|_| CryptoError::MalformedKeyId)?;
		Ok(Self { alg, digest })
	}
}

impl fmt::Display for KeyId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let label = alg_label(self.alg).unwrap_or("unknown");
		write!(f, "{label}:{}", hex::encode(self.digest))
	}
}

pub fn key_id(alg: u8, public_key: &[u8]) -> Result<KeyId, CryptoError> {
	if alg_label(alg).is_none() {
		return Err(CryptoError::UnsupportedAlgorithm(alg));
	}
	let mut hasher = Sha256::new();
	hasher.update([alg]);
	hasher.update(public_key);
	Ok(KeyId::from_digest(alg, hasher.finalize().into()))
}

#[derive(Clone)]
pub struct SigningKey(ed25519_dalek::SigningKey);

impl SigningKey {
	pub fn from_seed(seed: &[u8; 32]) -> Self {
		Self(ed25519_dalek::SigningKey::from_bytes(seed))
	}

	pub fn verifying_key(&self) -> VerifyingKey {
		VerifyingKey {
			alg: ALG_ED25519,
			inner: self.0.verifying_key(),
		}
	}

	pub fn key_id(&self) -> KeyId {
		key_id(ALG_ED25519, &self.0.verifying_key().to_bytes()).expect("Ed25519 is supported")
	}

	pub fn sign(&self, message: &[u8]) -> Vec<u8> {
		self.0.sign(message).to_bytes().to_vec()
	}
}

impl fmt::Debug for SigningKey {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("SigningKey")
			.field("key_id", &self.key_id())
			.finish_non_exhaustive()
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyingKey {
	alg: u8,
	inner: ed25519_dalek::VerifyingKey,
}

impl VerifyingKey {
	pub fn from_bytes(alg: u8, bytes: &[u8]) -> Result<Self, CryptoError> {
		if alg != ALG_ED25519 {
			return Err(CryptoError::UnsupportedAlgorithm(alg));
		}
		let array: [u8; 32] = bytes.try_into().map_err(|_| CryptoError::InvalidPublicKey)?;
		let inner = ed25519_dalek::VerifyingKey::from_bytes(&array).map_err(|_| CryptoError::InvalidPublicKey)?;
		Ok(Self { alg, inner })
	}

	pub const fn algorithm(self) -> u8 {
		self.alg
	}

	pub fn to_bytes(self) -> [u8; 32] {
		self.inner.to_bytes()
	}

	pub fn key_id(self) -> KeyId {
		key_id(self.alg, &self.inner.to_bytes()).expect("only supported algorithms are stored")
	}

	pub fn verify(self, message: &[u8], signature: &[u8]) -> Result<(), CryptoError> {
		let array: [u8; 64] = signature.try_into().map_err(|_| CryptoError::InvalidSignature)?;
		let signature = ed25519_dalek::Signature::from_bytes(&array);
		self.inner
			.verify(message, &signature)
			.map_err(|_| CryptoError::InvalidSignature)
	}
}
