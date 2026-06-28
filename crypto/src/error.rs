use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
	UnsupportedAlgorithm(u8),
	InvalidPublicKey,
	InvalidSignature,
	MalformedKeyId,
}

impl fmt::Display for CryptoError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::UnsupportedAlgorithm(alg) => write!(f, "unsupported signature algorithm 0x{alg:02x}"),
			Self::InvalidPublicKey => f.write_str("invalid Ed25519 public key"),
			Self::InvalidSignature => f.write_str("invalid Ed25519 signature"),
			Self::MalformedKeyId => f.write_str("malformed key id"),
		}
	}
}

impl std::error::Error for CryptoError {}
