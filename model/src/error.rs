use std::fmt;

use moraine_codec::CodecError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
	InvalidEncoding,
	NonCanonicalEncoding,
	DuplicateKey,
	UnknownField,
	MissingField,
	InvalidFieldType,
	InvalidFieldValue,
	UnknownCriticalExtension,
	ObjectIdMismatch,
	WrongObjectKind,
	WrongSubject,
	BadSignature,
	ThresholdNotMet,
	UnauthorizedKind,
	DuplicateSigner,
	GenesisKindRequirement,
	TransferNeedsTwoSignatures,
	CrossSignatureRequired,
	ProfileAuthority,
	PrimaryArtifactAmbiguous,
	UnknownPredicateScheme,
	DigestMismatch,
	SequenceGap,
	PreviousMismatch,
	Rollback,
	Fork,
}

impl RejectReason {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::InvalidEncoding => "invalid-encoding",
			Self::NonCanonicalEncoding => "non-canonical-encoding",
			Self::DuplicateKey => "duplicate-key",
			Self::UnknownField => "unknown-field",
			Self::MissingField => "missing-field",
			Self::InvalidFieldType => "invalid-field-type",
			Self::InvalidFieldValue => "invalid-field-value",
			Self::UnknownCriticalExtension => "unknown-critical-extension",
			Self::ObjectIdMismatch => "object-id-mismatch",
			Self::WrongObjectKind => "wrong-object-kind",
			Self::WrongSubject => "wrong-subject",
			Self::BadSignature => "bad-signature",
			Self::ThresholdNotMet => "threshold-not-met",
			Self::UnauthorizedKind => "unauthorized-kind",
			Self::DuplicateSigner => "duplicate-signer",
			Self::GenesisKindRequirement => "genesis-kind-requirement",
			Self::TransferNeedsTwoSignatures => "transfer-needs-two-signatures",
			Self::CrossSignatureRequired => "cross-signature-required",
			Self::ProfileAuthority => "profile-authority",
			Self::PrimaryArtifactAmbiguous => "primary-artifact-ambiguous",
			Self::UnknownPredicateScheme => "unknown-predicate-scheme",
			Self::DigestMismatch => "digest-mismatch",
			Self::SequenceGap => "sequence-gap",
			Self::PreviousMismatch => "previous-mismatch",
			Self::Rollback => "rollback",
			Self::Fork => "fork",
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelError {
	pub reason: RejectReason,
	pub detail: String,
}

impl ModelError {
	pub fn new(reason: RejectReason, detail: impl Into<String>) -> Self {
		Self {
			reason,
			detail: detail.into(),
		}
	}

	pub fn field(reason: RejectReason, key: &str) -> Self {
		Self::new(reason, format!("field `{key}`"))
	}

	pub fn from_codec(error: CodecError) -> Self {
		let reason = match error {
			CodecError::DuplicateKey => RejectReason::DuplicateKey,
			CodecError::UnsortedMapKeys
			| CodecError::NonMinimalInteger
			| CodecError::NonMinimalLength
			| CodecError::NormalizationKeyCollision => RejectReason::NonCanonicalEncoding,
			_ => RejectReason::InvalidEncoding,
		};
		Self::new(reason, error.to_string())
	}
}

impl fmt::Display for ModelError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}: {}", self.reason.as_str(), self.detail)
	}
}

impl std::error::Error for ModelError {}
