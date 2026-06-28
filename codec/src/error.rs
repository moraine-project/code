use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
	UnexpectedEof,
	TrailingBytes,
	UnsupportedSimple,
	FloatForbidden,
	TagForbidden,
	IndefiniteLengthForbidden,
	NonMinimalInteger,
	NonMinimalLength,
	DuplicateKey,
	UnsortedMapKeys,
	NormalizationKeyCollision,
	IntegerOutOfRange,
	InvalidUtf8,
	DepthLimitExceeded,
}

impl fmt::Display for CodecError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let message = match self {
			Self::UnexpectedEof => "unexpected end of input",
			Self::TrailingBytes => "trailing bytes after the top-level item",
			Self::UnsupportedSimple => "unsupported simple value",
			Self::FloatForbidden => "floating-point values are forbidden by the profile",
			Self::TagForbidden => "CBOR tags are not used by this profile",
			Self::IndefiniteLengthForbidden => "indefinite-length items are forbidden",
			Self::NonMinimalInteger => "integer is not encoded in its minimal width",
			Self::NonMinimalLength => "length is not encoded in its minimal width",
			Self::DuplicateKey => "duplicate map key",
			Self::UnsortedMapKeys => "map keys are not in deterministic order",
			Self::NormalizationKeyCollision => "text map keys differ only by Unicode normalization",
			Self::IntegerOutOfRange => "integer is outside the signed 64-bit range",
			Self::InvalidUtf8 => "text string is not valid UTF-8",
			Self::DepthLimitExceeded => "max nesting depth exceeded",
		};
		f.write_str(message)
	}
}

impl std::error::Error for CodecError {}
