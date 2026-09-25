use moraine_codec::Value;
use moraine_crypto::{KeyId, ObjectKind, SigningKey, VerifyingKey, alg_label, domain_tag, key_id, object_id_string};

use crate::canonical::{Canonical, Fields, expect_array, expect_bytes, expect_u32, map_of};
use crate::error::{ModelError, RejectReason};

pub const ALG_ED25519: u8 = 0x01;

pub trait ObjectPayload: Canonical {
	const KIND: ObjectKind;

	fn validate(&self) -> Result<(), ModelError>;
}

macro_rules! object_payload {
	($($type:ty => $kind:expr),+ $(,)?) => {
		$(
			impl ObjectPayload for $type {
				const KIND: ObjectKind = $kind;

				fn validate(&self) -> Result<(), ModelError> {
					<$type>::validate(self)
				}
			}
		)+
	};
}

object_payload! {
	crate::advisory::Advisory => ObjectKind::Advisory,
	crate::attestation::Attestation => ObjectKind::Attestation,
	crate::attestation::AttestationObject => ObjectKind::Attestation,
	crate::attestation::MirrorCommitment => ObjectKind::Attestation,
	crate::changelog::Changelog => ObjectKind::Changelog,
	crate::definition::GameDef => ObjectKind::GameDef,
	crate::definition::LoaderObject => ObjectKind::LoaderDef,
	crate::definition::RuntimeDef => ObjectKind::RuntimeDef,
	crate::delegation::Delegation => ObjectKind::Delegation,
	crate::delegation::KeyDelegation => ObjectKind::Delegation,
	crate::delegation::Migration => ObjectKind::Delegation,
	crate::delegation::OwnershipTransfer => ObjectKind::Delegation,
	crate::delegation::RecoveryEvent => ObjectKind::Delegation,
	crate::deny_list::DenyList => ObjectKind::DenyList,
	crate::feed::FeedEntry => ObjectKind::FeedEntry,
	crate::genesis::Genesis => ObjectKind::Genesis,
	crate::location::LocationRecord => ObjectKind::Release,
	crate::modpack::ModpackManifest => ObjectKind::Modpack,
	crate::profile::ProfileRevision => ObjectKind::Profile,
	crate::release::ReleaseObject => ObjectKind::Release,
	crate::release::ReleasePayload => ObjectKind::Release,
	crate::release::Withdrawal => ObjectKind::Release,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
	pub alg: u8,
	pub key_id: KeyId,
	pub sig: Vec<u8>,
}

impl Canonical for Signature {
	fn to_value(&self) -> Value {
		map_of(
			"Signature",
			[
				("alg", Value::int(i64::from(self.alg))),
				("key_id", Value::bytes(self.key_id.digest())),
				("sig", Value::bytes(self.sig.clone())),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Signature", value)?.reject_unknown(&["alg", "key_id", "sig"])?;
		let alg = expect_u32(fields.required("alg")?, "alg")?;
		let alg = u8::try_from(alg).map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, "alg"))?;
		if alg_label(alg).is_none() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "alg"));
		}
		let digest: [u8; 32] = expect_bytes(fields.required("key_id")?, "key_id")?
			.try_into()
			.map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, "key_id"))?;
		let sig = expect_bytes(fields.required("sig")?, "sig")?;
		Ok(Self {
			alg,
			key_id: KeyId::from_digest(alg, digest),
			sig,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureEnvelope {
	pub alg: u8,
	pub signatures: Vec<Signature>,
	pub key_ids: Vec<KeyId>,
}

impl Canonical for SignatureEnvelope {
	fn to_value(&self) -> Value {
		map_of(
			"SignatureEnvelope",
			[
				("alg", Value::int(i64::from(self.alg))),
				(
					"signatures",
					Value::array(self.signatures.iter().map(Canonical::to_value).collect::<Vec<_>>()),
				),
				(
					"key_ids",
					Value::array(self.key_ids.iter().map(|id| Value::bytes(id.digest())).collect::<Vec<_>>()),
				),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("SignatureEnvelope", value)?.reject_unknown(&["alg", "signatures", "key_ids"])?;
		let alg = expect_u32(fields.required("alg")?, "alg")?;
		let alg = u8::try_from(alg).map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, "alg"))?;
		if alg_label(alg).is_none() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "alg"));
		}
		let signatures = expect_array(fields.required("signatures")?, "signatures")?
			.iter()
			.cloned()
			.map(Signature::from_value)
			.collect::<Result<Vec<_>, _>>()?;
		let key_ids = expect_array(fields.required("key_ids")?, "key_ids")?
			.iter()
			.map(|value| {
				let digest: [u8; 32] = expect_bytes(value, "key_ids")?
					.try_into()
					.map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, "key_ids"))?;
				Ok(KeyId::from_digest(alg, digest))
			})
			.collect::<Result<Vec<_>, ModelError>>()?;
		Ok(Self {
			alg,
			signatures,
			key_ids,
		})
	}
}

pub struct SignedObject<T> {
	pub envelope: SignatureEnvelope,
	pub payload_bytes: Vec<u8>,
	pub payload: T,
}

impl<T: Canonical> SignedObject<T> {
	pub fn from_parts(envelope: SignatureEnvelope, payload: T) -> Self {
		let payload_bytes = payload.to_canonical_bytes();
		Self {
			envelope,
			payload_bytes,
			payload,
		}
	}

	pub fn from_raw_payload(payload_bytes: Vec<u8>, envelope: SignatureEnvelope) -> Result<Self, ModelError> {
		let payload = T::from_canonical_bytes(&payload_bytes)?;
		Ok(Self {
			envelope,
			payload_bytes,
			payload,
		})
	}

	pub fn from_bytes(bytes: &[u8]) -> Result<Self, ModelError> {
		let value = moraine_codec::decode(bytes).map_err(ModelError::from_codec)?;
		let fields = Fields::new("SignedObject", value)?.reject_unknown(&["envelope", "payload"])?;
		let envelope = SignatureEnvelope::from_value(fields.required("envelope")?.clone())?;
		let payload_bytes = expect_bytes(fields.required("payload")?, "payload")?;
		let payload = T::from_canonical_bytes(&payload_bytes)?;
		Ok(Self {
			envelope,
			payload_bytes,
			payload,
		})
	}

	pub fn signed_message(&self, kind: ObjectKind) -> Vec<u8> {
		let mut message = domain_tag(kind);
		message.extend_from_slice(&self.payload_bytes);
		message
	}

	pub fn id(&self, kind: ObjectKind) -> String {
		object_id_string(kind, &self.payload_bytes)
	}

	pub fn wire_bytes(&self) -> Vec<u8> {
		let outer = map_of(
			"SignedObject",
			[
				("envelope", self.envelope.to_value()),
				("payload", Value::bytes(self.payload_bytes.clone())),
			],
		);
		moraine_codec::encode(&outer).expect("signed object is encodable")
	}

	pub fn verify_threshold(&self, kind: ObjectKind, trusted: &[TrustedKey], threshold: usize) -> Result<usize, ModelError> {
		verify_envelope(&self.envelope, &self.signed_message(kind), trusted, threshold)
	}
}

pub fn try_sign_payload<T: ObjectPayload + Clone>(
	kind: ObjectKind,
	payload: &T,
	signers: &[&SigningKey],
) -> Result<SignedObject<T>, ModelError> {
	if signers.is_empty() {
		return Err(ModelError::new(
			RejectReason::InvalidFieldValue,
			"at least one signer is required",
		));
	}
	let mut signer_ids = std::collections::HashSet::new();
	if signers.iter().any(|signer| !signer_ids.insert(signer.key_id())) {
		return Err(ModelError::new(
			RejectReason::DuplicateSigner,
			"a signer may appear only once",
		));
	}
	if kind != T::KIND {
		return Err(ModelError::new(
			RejectReason::WrongObjectKind,
			format!("payload belongs to `{}`, not `{}`", T::KIND.as_str(), kind.as_str()),
		));
	}
	payload.validate()?;
	let value = payload.to_value();
	let checked = T::from_value(value)?;
	let payload_bytes = checked.to_canonical_bytes();
	let mut message = domain_tag(kind);
	message.extend_from_slice(&payload_bytes);
	let (alg, signatures) = sign_message(&message, signers);
	let key_ids = signers.iter().map(|signer| signer.key_id()).collect();
	Ok(SignedObject {
		envelope: SignatureEnvelope {
			alg,
			signatures,
			key_ids,
		},
		payload_bytes,
		payload: checked,
	})
}

pub fn sign_payload<T: ObjectPayload + Clone>(
	kind: ObjectKind,
	payload: &T,
	signers: &[&SigningKey],
) -> Result<SignedObject<T>, ModelError> {
	try_sign_payload(kind, payload, signers)
}

pub(crate) fn sign_message(message: &[u8], signers: &[&SigningKey]) -> (u8, Vec<Signature>) {
	let signatures = signers
		.iter()
		.map(|signer| Signature {
			alg: ALG_ED25519,
			key_id: signer.key_id(),
			sig: signer.sign(message),
		})
		.collect();
	(ALG_ED25519, signatures)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedKey {
	pub key_id: KeyId,
	pub key: VerifyingKey,
}

impl TrustedKey {
	pub fn new(public_key: &[u8]) -> Result<Self, ModelError> {
		let key = VerifyingKey::from_bytes(ALG_ED25519, public_key)
			.map_err(|_| ModelError::new(RejectReason::InvalidFieldValue, "invalid public key"))?;
		let id = key_id(ALG_ED25519, public_key)
			.map_err(|_| ModelError::new(RejectReason::InvalidFieldValue, "invalid public key"))?;
		Ok(Self { key_id: id, key })
	}
}

pub fn verify_envelope(
	envelope: &SignatureEnvelope,
	message: &[u8],
	trusted: &[TrustedKey],
	threshold: usize,
) -> Result<usize, ModelError> {
	let mut distinct = Vec::new();
	for signature in &envelope.signatures {
		let trusted_key = trusted
			.iter()
			.find(|candidate| candidate.key_id == signature.key_id)
			.ok_or_else(|| ModelError::new(RejectReason::BadSignature, format!("unknown key {}", signature.key_id)))?;
		trusted_key.key.verify(message, &signature.sig).map_err(|_| {
			ModelError::new(
				RejectReason::BadSignature,
				format!("signature from {} did not verify", signature.key_id),
			)
		})?;
		if distinct.contains(&signature.key_id) {
			return Err(ModelError::new(RejectReason::DuplicateSigner, signature.key_id.to_string()));
		}
		distinct.push(signature.key_id);
	}

	let envelope_ids: Vec<KeyId> = envelope.key_ids.clone();
	if envelope_ids.len() != distinct.len() || !envelope_ids.iter().all(|id| distinct.contains(id)) {
		return Err(ModelError::new(
			RejectReason::InvalidFieldValue,
			"envelope key_ids do not match the participating signatures",
		));
	}

	if distinct.len() < threshold {
		return Err(ModelError::new(
			RejectReason::ThresholdNotMet,
			format!("{} valid signatures for threshold {threshold}", distinct.len()),
		));
	}
	Ok(distinct.len())
}

#[cfg(test)]
mod tests {
	use moraine_crypto::{ObjectKind, SigningKey};

	use super::*;
	use crate::artifact::Artifact;
	use crate::definition::{GameDef, VersionSyntax};
	use crate::release::{ReleasePayload, Withdrawal};

	fn artifact(size: u64) -> Artifact {
		Artifact {
			digest: vec![0xAB; 32],
			size,
			media_type: "application/octet-stream".to_string(),
			filename: "artifact.bin".to_string(),
			is_primary: true,
			os_predicate: None,
			arch_predicate: None,
		}
	}

	fn release(size: u64) -> ReleasePayload {
		ReleasePayload {
			protocol: 1,
			project_id: "gd:sha256:project".to_string(),
			game_id: "gd:sha256:game".to_string(),
			release_nonce: vec![0x11; 16],
			human_version: "1.0.0".to_string(),
			channel: "release".to_string(),
			kind: "mod".to_string(),
			declared_time: 1_760_000_000,
			compatibility: Vec::new(),
			artifacts: vec![artifact(size)],
			dependencies: Vec::new(),
			source_reference: None,
			changelog_digest: None,
			license_expression: None,
			rights: None,
			sbom_digest: None,
			minimum_verifier_version: 1,
			critical_extensions: Vec::new(),
		}
	}

	fn game_def() -> GameDef {
		GameDef {
			protocol: 1,
			game_id: "gd:sha256:game".to_string(),
			display_name: "Example".to_string(),
			version_syntax: VersionSyntax {
				kind: "semver".to_string(),
				pattern: None,
			},
			version_ordering: "semver".to_string(),
			version_catalog: vec!["1.0.0".to_string()],
			loaders_allowed: true,
			loader_authorities: Vec::new(),
			categories: Vec::new(),
			tags: Vec::new(),
			metadata_extractor: None,
			install_adapter: None,
			declared_time: 1_760_000_000,
		}
	}

	fn withdrawal() -> Withdrawal {
		Withdrawal {
			protocol: 1,
			project_id: "gd:sha256:project".to_string(),
			release_id: "gd:sha256:release".to_string(),
			reason: "broken".to_string(),
			note: None,
			declared_time: 1_760_000_000,
		}
	}

	#[test]
	fn checked_signing_rejects_invalid_payloads_and_kind_mismatches() {
		let key = SigningKey::from_seed(&[1; 32]);
		let mut invalid_release = release(10);
		assert!(try_sign_payload(ObjectKind::Release, &invalid_release, &[&key]).is_ok());
		invalid_release.protocol = 2;
		let error = try_sign_payload(ObjectKind::Release, &invalid_release, &[&key])
			.err()
			.expect("invalid release");
		assert_eq!(error.reason, RejectReason::InvalidFieldValue);
		let error = sign_payload(ObjectKind::Release, &invalid_release, &[&key])
			.err()
			.expect("invalid release");
		assert_eq!(error.reason, RejectReason::InvalidFieldValue);

		let mut invalid_game = game_def();
		invalid_game.version_ordering = "not-a-scheme".to_string();
		let error = sign_payload(ObjectKind::GameDef, &invalid_game, &[&key])
			.err()
			.expect("invalid game definition");
		assert_eq!(error.reason, RejectReason::InvalidFieldValue);

		let error = try_sign_payload(ObjectKind::Profile, &withdrawal(), &[&key])
			.err()
			.expect("kind mismatch");
		assert_eq!(error.reason, RejectReason::WrongObjectKind);
	}

	#[test]
	fn checked_signing_rejects_an_oversized_u64_without_panicking() {
		let key = SigningKey::from_seed(&[1; 32]);
		let payload = release(u64::MAX);
		let error = try_sign_payload(ObjectKind::Release, &payload, &[&key])
			.err()
			.expect("oversized release");
		assert_eq!(error.reason, RejectReason::InvalidFieldValue);
		let error = sign_payload(ObjectKind::Release, &payload, &[&key])
			.err()
			.expect("oversized release");
		assert_eq!(error.reason, RejectReason::InvalidFieldValue);
	}
}
