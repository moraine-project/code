use moraine_codec::decode;
use moraine_crypto::{KeyId, ObjectKind, object_id_string};
use moraine_model::Canonical;
use moraine_model::advisory::Advisory;
use moraine_model::attestation::AttestationObject;
use moraine_model::changelog::Changelog;
use moraine_model::compatibility::{Predicate, PredicateResult};
use moraine_model::definition::{GameDef, LoaderObject, RuntimeDef};
use moraine_model::delegation::{Delegation, DelegationPurpose};
use moraine_model::deny_list::DenyList;
use moraine_model::error::{ModelError, RejectReason};
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::{Genesis, GenesisKind};
use moraine_model::modpack::ModpackManifest;
use moraine_model::profile::ProfileRevision;
use moraine_model::release::ReleaseObject;
use moraine_model::signed::{Signature, SignatureEnvelope, SignedObject, TrustedKey, verify_envelope};
use moraine_model::trust::{RootSet, verify_key_delegation, verify_migration, verify_ownership_transfer, verify_recovery};
use moraine_model::version::{OrderingScheme, VersionCatalog};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorFile {
	pub protocol: u32,
	pub description: String,
	pub vectors: Vec<Vector>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector {
	pub name: String,
	pub category: String,
	pub kind: String,
	pub payload_hex: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub envelope: Option<EnvelopeJson>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub expected_id: Option<String>,
	pub expected_verdict: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub reason_code: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub verify_kind: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub trust: Option<TrustJson>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub prior_feed_hex: Vec<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub predicate_case: Option<PredicateCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredicateCase {
	pub ordering: String,
	#[serde(default)]
	pub catalog: Vec<String>,
	pub scheme: String,
	#[serde(default)]
	pub values: Vec<String>,
	pub version: String,
	pub expected: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeJson {
	pub alg: u8,
	pub signatures: Vec<SignatureJson>,
	pub key_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureJson {
	pub alg: u8,
	pub key_id: String,
	pub sig: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustJson {
	pub roots: Vec<String>,
	pub threshold: usize,
	#[serde(default)]
	pub authorized_kinds: Vec<String>,
	#[serde(default)]
	pub profile_roots: Vec<String>,
	#[serde(default)]
	pub delegated_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actual {
	pub accepted: bool,
	pub reason_code: Option<&'static str>,
}

impl Actual {
	fn accept() -> Self {
		Self {
			accepted: true,
			reason_code: None,
		}
	}

	fn reject(reason: RejectReason) -> Self {
		Self {
			accepted: false,
			reason_code: Some(reason.as_str()),
		}
	}

	fn from_error(error: ModelError) -> Self {
		Self::reject(error.reason)
	}
}

pub fn evaluate(vector: &Vector) -> Actual {
	if vector.kind == "predicate" {
		return evaluate_predicate(vector);
	}
	if vector.kind == "canonical" {
		return match hex::decode(&vector.payload_hex) {
			Ok(bytes) => match decode(&bytes) {
				Ok(_) => Actual::accept(),
				Err(error) => Actual::from_error(ModelError::from_codec(error)),
			},
			Err(_) => Actual::reject(RejectReason::InvalidEncoding),
		};
	}

	let Some(kind) = ObjectKind::parse(&vector.kind) else {
		return Actual::reject(RejectReason::WrongObjectKind);
	};
	let Ok(payload) = hex::decode(&vector.payload_hex) else {
		return Actual::reject(RejectReason::InvalidEncoding);
	};
	let Some(envelope) = &vector.envelope else {
		return Actual::reject(RejectReason::InvalidEncoding);
	};
	let Ok(envelope) = envelope.to_envelope() else {
		return Actual::reject(RejectReason::InvalidEncoding);
	};
	let verify_kind = vector.verify_kind.as_deref().and_then(ObjectKind::parse).unwrap_or(kind);

	let outcome = match kind {
		ObjectKind::Genesis => verify_genesis_object(vector, payload, &envelope, verify_kind),
		ObjectKind::Delegation => verify_delegation_object(vector, payload, &envelope, verify_kind),
		ObjectKind::Release => verify_with_trust::<ReleaseObject>(vector, payload, &envelope, kind, verify_kind),
		ObjectKind::Profile => verify_profile_object(vector, payload, &envelope, verify_kind),
		ObjectKind::FeedEntry => verify_feed_object(vector, payload, &envelope, kind, verify_kind),
		ObjectKind::GameDef => verify_with_trust::<GameDef>(vector, payload, &envelope, kind, verify_kind),
		ObjectKind::LoaderDef => verify_with_trust::<LoaderObject>(vector, payload, &envelope, kind, verify_kind),
		ObjectKind::RuntimeDef => verify_with_trust::<RuntimeDef>(vector, payload, &envelope, kind, verify_kind),
		ObjectKind::Attestation => verify_with_trust::<AttestationObject>(vector, payload, &envelope, kind, verify_kind),
		ObjectKind::Advisory => verify_with_trust::<Advisory>(vector, payload, &envelope, kind, verify_kind),
		ObjectKind::Changelog => verify_with_trust::<Changelog>(vector, payload, &envelope, kind, verify_kind),
		ObjectKind::Modpack => verify_with_trust::<ModpackManifest>(vector, payload, &envelope, kind, verify_kind),
		ObjectKind::DenyList => verify_with_trust::<DenyList>(vector, payload, &envelope, kind, verify_kind),
	};

	match outcome {
		Ok(()) => Actual::accept(),
		Err(error) => Actual::from_error(error),
	}
}

fn evaluate_predicate(vector: &Vector) -> Actual {
	let Some(case) = &vector.predicate_case else {
		return Actual::reject(RejectReason::InvalidFieldValue);
	};
	let Some(scheme) = OrderingScheme::parse(&case.ordering) else {
		return Actual::reject(RejectReason::InvalidFieldValue);
	};
	let catalog = VersionCatalog::new(scheme, case.catalog.clone());
	let predicate = Predicate {
		scheme: case.scheme.clone(),
		values: case.values.clone(),
	};
	let token = match catalog.evaluate(&predicate, &case.version) {
		PredicateResult::Satisfied => "satisfied",
		PredicateResult::NotSatisfied => "not-satisfied",
		PredicateResult::Unknown => "unknown",
	};
	if token == case.expected {
		Actual::accept()
	} else {
		Actual::reject(RejectReason::InvalidFieldValue)
	}
}

fn payload_subject(payload: &Delegation) -> Result<String, ModelError> {
	match payload {
		Delegation::Key(delegation) => Ok(delegation.project_id.clone()),
		Delegation::OwnershipTransfer(transfer) => Ok(transfer.project_id.clone()),
		Delegation::Migration(migration) => Ok(migration.project_id.clone()),
		Delegation::Recovery(recovery) => Ok(recovery.project_id.clone()),
	}
}

fn verify_genesis_object(
	vector: &Vector,
	payload: Vec<u8>,
	envelope: &SignatureEnvelope,
	verify_kind: ObjectKind,
) -> Result<(), ModelError> {
	let signed = SignedObject::<Genesis>::from_raw_payload(payload, envelope.clone())?;
	check_expected_id(vector, ObjectKind::Genesis, &signed.payload_bytes)?;
	let message = signed.signed_message(verify_kind);
	let root = RootSet::from_genesis(&signed.payload, &signed.id(ObjectKind::Genesis))?;
	root.verify(&message, envelope)?;
	Ok(())
}

fn verify_delegation_object(
	vector: &Vector,
	payload: Vec<u8>,
	envelope: &SignatureEnvelope,
	verify_kind: ObjectKind,
) -> Result<(), ModelError> {
	let signed = SignedObject::<Delegation>::from_raw_payload(payload, envelope.clone())?;
	check_expected_id(vector, ObjectKind::Delegation, &signed.payload_bytes)?;
	let trust = vector
		.trust
		.as_ref()
		.expect("trusted keys are required for delegation vectors");
	let root = trust.to_root_set(&payload_subject(&signed.payload)?)?;
	if verify_kind != ObjectKind::Delegation {
		let message = signed.signed_message(verify_kind);
		root.verify(&message, envelope)?;
		return Ok(());
	}
	match signed.payload.purpose() {
		DelegationPurpose::Key => verify_key_delegation(&signed, &root),
		DelegationPurpose::OwnershipTransfer => verify_ownership_transfer(&signed, &root),
		DelegationPurpose::Migration => verify_migration(&signed, &root),
		DelegationPurpose::Recovery => verify_recovery(&signed, &root),
	}
}

fn verify_profile_object(
	vector: &Vector,
	payload: Vec<u8>,
	envelope: &SignatureEnvelope,
	verify_kind: ObjectKind,
) -> Result<(), ModelError> {
	let signed = SignedObject::<ProfileRevision>::from_raw_payload(payload, envelope.clone())?;
	check_expected_id(vector, ObjectKind::Profile, &signed.payload_bytes)?;
	let trust = vector.trust.as_ref().expect("trusted keys are required for profile vectors");
	let message = signed.signed_message(verify_kind);
	let roots = trust.trusted_keys()?;
	let profile_keys = trust.profile_trusted_keys()?;
	if verify_envelope(envelope, &message, &profile_keys, trust.threshold).is_ok() {
		return Ok(());
	}
	if verify_envelope(envelope, &message, &roots, trust.threshold).is_ok() {
		return Err(ModelError::new(
			RejectReason::ProfileAuthority,
			"signer holds a release-only delegation",
		));
	}
	let delegated = trust.delegated_trusted_keys()?;
	if verify_envelope(envelope, &message, &delegated, trust.threshold).is_ok() {
		return Err(ModelError::new(
			RejectReason::ProfileAuthority,
			"signer does not hold a profile delegation",
		));
	}
	Err(ModelError::new(
		RejectReason::BadSignature,
		"no valid signature under the trusted keys",
	))
}

fn verify_with_trust<T: moraine_model::Canonical>(
	vector: &Vector,
	payload: Vec<u8>,
	envelope: &SignatureEnvelope,
	kind: ObjectKind,
	verify_kind: ObjectKind,
) -> Result<(), ModelError> {
	let signed = SignedObject::<T>::from_raw_payload(payload, envelope.clone())?;
	check_expected_id(vector, kind, &signed.payload_bytes)?;
	let trust = vector.trust.as_ref().expect("trusted keys are required");
	let message = signed.signed_message(verify_kind);
	verify_envelope(envelope, &message, &trust.trusted_keys()?, trust.threshold)?;
	Ok(())
}

fn verify_feed_object(
	vector: &Vector,
	payload: Vec<u8>,
	envelope: &SignatureEnvelope,
	kind: ObjectKind,
	verify_kind: ObjectKind,
) -> Result<(), ModelError> {
	let signed = SignedObject::<FeedEntry>::from_raw_payload(payload, envelope.clone())?;
	check_expected_id(vector, kind, &signed.payload_bytes)?;
	for prior in &vector.prior_feed_hex {
		let bytes = hex::decode(prior).map_err(|_| ModelError::new(RejectReason::InvalidEncoding, "prior feed"))?;
		let prior = FeedEntry::from_canonical_bytes(&bytes)?;
		signed.payload.verify_follows(&prior)?;
	}
	let trust = vector.trust.as_ref().expect("trusted keys are required");
	let message = signed.signed_message(verify_kind);
	verify_envelope(envelope, &message, &trust.trusted_keys()?, trust.threshold)?;
	Ok(())
}

fn check_expected_id(vector: &Vector, kind: ObjectKind, payload_bytes: &[u8]) -> Result<(), ModelError> {
	if let Some(expected) = &vector.expected_id
		&& object_id_string(kind, payload_bytes) != *expected
	{
		return Err(ModelError::new(
			RejectReason::ObjectIdMismatch,
			"computed id does not match expected_id",
		));
	}
	Ok(())
}

impl EnvelopeJson {
	fn to_envelope(&self) -> Result<SignatureEnvelope, ModelError> {
		let signatures = self
			.signatures
			.iter()
			.map(|signature| {
				let digest: [u8; 32] = hex::decode(&signature.key_id)
					.map_err(|_| ModelError::new(RejectReason::InvalidEncoding, "key_id hex"))?
					.try_into()
					.map_err(|_| ModelError::new(RejectReason::InvalidEncoding, "key_id length"))?;
				Ok(Signature {
					alg: signature.alg,
					key_id: KeyId::from_digest(signature.alg, digest),
					sig: hex::decode(&signature.sig)
						.map_err(|_| ModelError::new(RejectReason::InvalidEncoding, "sig hex"))?,
				})
			})
			.collect::<Result<Vec<_>, ModelError>>()?;
		let key_ids = self
			.key_ids
			.iter()
			.map(|key_id| {
				let digest: [u8; 32] = hex::decode(key_id)
					.map_err(|_| ModelError::new(RejectReason::InvalidEncoding, "key_ids hex"))?
					.try_into()
					.map_err(|_| ModelError::new(RejectReason::InvalidEncoding, "key_ids length"))?;
				Ok(KeyId::from_digest(self.alg, digest))
			})
			.collect::<Result<Vec<_>, ModelError>>()?;
		Ok(SignatureEnvelope {
			alg: self.alg,
			signatures,
			key_ids,
		})
	}
}

impl TrustJson {
	fn trusted_keys(&self) -> Result<Vec<TrustedKey>, ModelError> {
		self.roots.iter().map(|root| decode_key(root)).collect()
	}

	fn profile_trusted_keys(&self) -> Result<Vec<TrustedKey>, ModelError> {
		self.profile_roots.iter().map(|root| decode_key(root)).collect()
	}

	fn delegated_trusted_keys(&self) -> Result<Vec<TrustedKey>, ModelError> {
		self.delegated_keys.iter().map(|key| decode_key(key)).collect()
	}

	fn to_root_set(&self, subject: &str) -> Result<RootSet, ModelError> {
		let kinds = if self.authorized_kinds.is_empty() {
			all_kind_strings()
		} else {
			self.authorized_kinds.clone()
		};
		let public_keys = self
			.roots
			.iter()
			.map(|root| hex::decode(root).map_err(|_| ModelError::new(RejectReason::InvalidEncoding, "root hex")))
			.collect::<Result<Vec<_>, _>>()?;
		RootSet::new(subject, &public_keys, self.threshold, kinds, GenesisKind::Project)
	}
}

fn decode_key(hex_key: &str) -> Result<TrustedKey, ModelError> {
	let bytes = hex::decode(hex_key).map_err(|_| ModelError::new(RejectReason::InvalidEncoding, "public key hex"))?;
	TrustedKey::new(&bytes)
}

fn all_kind_strings() -> Vec<String> {
	GenesisKind::Project
		.authority_kinds()
		.iter()
		.map(|kind| kind.as_str().to_string())
		.collect()
}

pub fn check(vector: &Vector) -> Result<(), String> {
	let actual = evaluate(vector);
	let expected_accept = vector.expected_verdict == "accept";
	if actual.accepted != expected_accept {
		return Err(format!(
			"expected {}, got {}",
			if expected_accept { "accept" } else { "reject" },
			if actual.accepted { "accept" } else { "reject" }
		));
	}
	if !expected_accept
		&& let Some(expected) = &vector.reason_code
		&& actual.reason_code != Some(expected.as_str())
	{
		return Err(format!("expected reason `{expected}`, got `{:?}`", actual.reason_code));
	}
	Ok(())
}
