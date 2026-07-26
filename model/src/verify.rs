use std::fmt;

use moraine_crypto::{ObjectKind, object_id};

use crate::advisory::Advisory;
use crate::attestation::AttestationObject;
use crate::canonical::Canonical;
use crate::definition::{GameDef, LoaderObject, RuntimeDef};
use crate::delegation::{Delegation, KeyDelegation};
use crate::error::ModelError;
use crate::feed::FeedEntry;
use crate::genesis::Genesis;
use crate::modpack::ModpackManifest;
use crate::profile::ProfileRevision;
use crate::release::ReleaseObject;
use crate::signed::{SignedObject, TrustedKey, verify_envelope};
use crate::trust::{RootSet, verify_genesis as verify_genesis_signed};

pub struct VerifiedObject {
	pub kind: ObjectKind,
	pub digest: [u8; 32],
	pub id: String,
	pub payload_bytes: Vec<u8>,
	pub wire_bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum VerifyError {
	Decode(ModelError),
	Signature(ModelError),
	UnsupportedKind(ObjectKind),
}

impl fmt::Display for VerifyError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Decode(error) => write!(f, "invalid object: {error}"),
			Self::Signature(error) => write!(f, "signature rejected: {error}"),
			Self::UnsupportedKind(kind) => write!(f, "object kind `{}` is not accepted yet", kind.as_str()),
		}
	}
}

impl std::error::Error for VerifyError {}

pub fn verify_genesis(wire: &[u8]) -> Result<(RootSet, VerifiedObject), VerifyError> {
	let signed = SignedObject::<Genesis>::from_bytes(wire).map_err(VerifyError::Decode)?;
	let root = verify_genesis_signed(&signed).map_err(VerifyError::Signature)?;
	let object = VerifiedObject {
		kind: ObjectKind::Genesis,
		digest: object_id(ObjectKind::Genesis, &signed.payload_bytes),
		id: signed.id(ObjectKind::Genesis),
		payload_bytes: signed.payload_bytes.clone(),
		wire_bytes: wire.to_vec(),
	};
	Ok((root, object))
}

pub fn verify_object(kind: ObjectKind, wire: &[u8], root: &RootSet) -> Result<VerifiedObject, VerifyError> {
	match kind {
		ObjectKind::Delegation => verify_typed::<Delegation>(kind, wire, root),
		ObjectKind::Release => verify_typed::<ReleaseObject>(kind, wire, root),
		ObjectKind::Profile => verify_typed::<ProfileRevision>(kind, wire, root),
		ObjectKind::FeedEntry => verify_typed::<FeedEntry>(kind, wire, root),
		ObjectKind::Advisory => verify_typed::<Advisory>(kind, wire, root),
		ObjectKind::Attestation => verify_typed::<AttestationObject>(kind, wire, root),
		ObjectKind::GameDef => verify_typed::<GameDef>(kind, wire, root),
		ObjectKind::LoaderDef => verify_typed::<LoaderObject>(kind, wire, root),
		ObjectKind::RuntimeDef => verify_typed::<RuntimeDef>(kind, wire, root),
		ObjectKind::Modpack => verify_typed::<ModpackManifest>(kind, wire, root),
		other => Err(VerifyError::UnsupportedKind(other)),
	}
}

pub fn verify_object_authorized(
	kind: ObjectKind,
	wire: &[u8],
	root: &RootSet,
	delegations: &[KeyDelegation],
	now: i64,
) -> Result<VerifiedObject, VerifyError> {
	match verify_object(kind, wire, root) {
		Ok(object) => Ok(object),
		Err(VerifyError::Signature(error)) => match kind {
			ObjectKind::Delegation => verify_delegated::<Delegation>(kind, wire, delegations, now, error),
			ObjectKind::Release => verify_delegated::<ReleaseObject>(kind, wire, delegations, now, error),
			ObjectKind::Profile => verify_delegated::<ProfileRevision>(kind, wire, delegations, now, error),
			ObjectKind::FeedEntry => verify_delegated::<FeedEntry>(kind, wire, delegations, now, error),
			ObjectKind::Advisory => verify_delegated::<Advisory>(kind, wire, delegations, now, error),
			ObjectKind::Attestation => verify_delegated::<AttestationObject>(kind, wire, delegations, now, error),
			ObjectKind::GameDef => verify_delegated::<GameDef>(kind, wire, delegations, now, error),
			ObjectKind::LoaderDef => verify_delegated::<LoaderObject>(kind, wire, delegations, now, error),
			ObjectKind::RuntimeDef => verify_delegated::<RuntimeDef>(kind, wire, delegations, now, error),
			ObjectKind::Modpack => verify_delegated::<ModpackManifest>(kind, wire, delegations, now, error),
			other => Err(VerifyError::UnsupportedKind(other)),
		},
		Err(other) => Err(other),
	}
}

fn verify_delegated<T: Canonical>(
	kind: ObjectKind,
	wire: &[u8],
	delegations: &[KeyDelegation],
	now: i64,
	root_error: ModelError,
) -> Result<VerifiedObject, VerifyError> {
	let signed = SignedObject::<T>::from_bytes(wire).map_err(VerifyError::Decode)?;
	let message = signed.signed_message(kind);
	for delegation in delegations {
		if !delegation.allowed_kinds.iter().any(|allowed| allowed == kind.as_str()) {
			continue;
		}
		if delegation.expires_at.is_some_and(|expires| expires <= now) {
			continue;
		}
		let trusted = TrustedKey::new(&delegation.delegate_key.public_key).map_err(VerifyError::Signature)?;
		if verify_envelope(&signed.envelope, &message, std::slice::from_ref(&trusted), 1).is_ok() {
			return Ok(VerifiedObject {
				kind,
				digest: object_id(kind, &signed.payload_bytes),
				id: signed.id(kind),
				payload_bytes: signed.payload_bytes.clone(),
				wire_bytes: wire.to_vec(),
			});
		}
	}
	Err(VerifyError::Signature(root_error))
}

fn verify_typed<T: Canonical>(kind: ObjectKind, wire: &[u8], root: &RootSet) -> Result<VerifiedObject, VerifyError> {
	let signed = SignedObject::<T>::from_bytes(wire).map_err(VerifyError::Decode)?;
	signed
		.verify_threshold(kind, root.keys(), root.threshold())
		.map_err(VerifyError::Signature)?;
	Ok(VerifiedObject {
		kind,
		digest: object_id(kind, &signed.payload_bytes),
		id: signed.id(kind),
		payload_bytes: signed.payload_bytes.clone(),
		wire_bytes: wire.to_vec(),
	})
}
