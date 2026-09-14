use moraine_crypto::ObjectKind;

use crate::delegation::Delegation;
use crate::error::{ModelError, RejectReason};
use crate::genesis::{Genesis, GenesisKind};
use crate::signed::{SignedObject, TrustedKey, verify_envelope};

#[derive(Debug, Clone)]
pub struct RootSet {
	keys: Vec<TrustedKey>,
	threshold: usize,
	authorized_kinds: Vec<String>,
	genesis_kind: GenesisKind,
}

impl RootSet {
	pub fn new(
		public_keys: &[Vec<u8>],
		threshold: usize,
		authorized_kinds: Vec<String>,
		genesis_kind: GenesisKind,
	) -> Result<Self, ModelError> {
		let keys = public_keys
			.iter()
			.map(|public_key| TrustedKey::new(public_key))
			.collect::<Result<Vec<_>, _>>()?;
		if keys.is_empty() || threshold == 0 || threshold > keys.len() {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"a root set needs at least one key and a threshold within it",
			));
		}
		Ok(Self {
			keys,
			threshold,
			authorized_kinds,
			genesis_kind,
		})
	}

	pub fn from_genesis(genesis: &Genesis) -> Result<Self, ModelError> {
		genesis.validate()?;
		let keys = genesis
			.roots
			.iter()
			.map(|root| TrustedKey::new(&root.public_key))
			.collect::<Result<Vec<_>, _>>()?;
		Ok(Self {
			keys,
			threshold: usize::try_from(genesis.threshold).unwrap_or(usize::MAX),
			authorized_kinds: genesis.authorized_kinds.clone(),
			genesis_kind: genesis.kind,
		})
	}

	pub const fn threshold(&self) -> usize {
		self.threshold
	}

	pub const fn genesis_kind(&self) -> GenesisKind {
		self.genesis_kind
	}

	pub fn authorizes_kind(&self, kind: &str) -> bool {
		self.authorized_kinds.iter().any(|authorized| authorized == kind)
	}

	pub fn keys(&self) -> &[TrustedKey] {
		&self.keys
	}

	pub fn verify(&self, message: &[u8], envelope: &crate::signed::SignatureEnvelope) -> Result<usize, ModelError> {
		verify_envelope(envelope, message, &self.keys, self.threshold)
	}
}

pub fn verify_genesis(signed: &SignedObject<Genesis>) -> Result<RootSet, ModelError> {
	let root = RootSet::from_genesis(&signed.payload)?;
	let message = signed.signed_message(ObjectKind::Genesis);
	root.verify(&message, &signed.envelope)?;
	Ok(root)
}

pub fn verify_key_delegation(signed: &SignedObject<Delegation>, root: &RootSet) -> Result<(), ModelError> {
	let Delegation::Key(delegation) = &signed.payload else {
		return Err(ModelError::new(RejectReason::WrongObjectKind, "expected a key delegation"));
	};
	if !root.authorizes_kind("delegation") {
		return Err(ModelError::new(
			RejectReason::UnauthorizedKind,
			"genesis does not authorize delegations",
		));
	}
	for kind in &delegation.allowed_kinds {
		if !root.authorizes_kind(kind) {
			return Err(ModelError::new(
				RejectReason::UnauthorizedKind,
				format!("delegation grants `{kind}`, which genesis does not authorize"),
			));
		}
	}
	let message = signed.signed_message(ObjectKind::Delegation);
	root.verify(&message, &signed.envelope)?;
	Ok(())
}

pub fn verify_ownership_transfer(signed: &SignedObject<Delegation>, root: &RootSet) -> Result<(), ModelError> {
	if !matches!(signed.payload, Delegation::OwnershipTransfer(_)) {
		return Err(ModelError::new(
			RejectReason::WrongObjectKind,
			"expected an ownership transfer",
		));
	}
	let message = signed.signed_message(ObjectKind::Delegation);
	let valid = verify_envelope(&signed.envelope, &message, root.keys(), root.threshold())?;
	if valid < 2 {
		return Err(ModelError::new(
			RejectReason::TransferNeedsTwoSignatures,
			"ownership transfer requires the previous and new owner signatures",
		));
	}
	Ok(())
}

pub fn verify_migration(signed: &SignedObject<Delegation>, root: &RootSet) -> Result<(), ModelError> {
	if !matches!(signed.payload, Delegation::Migration(_)) {
		return Err(ModelError::new(RejectReason::WrongObjectKind, "expected a migration record"));
	}
	let message = signed.signed_message(ObjectKind::Delegation);
	let valid = verify_envelope(&signed.envelope, &message, root.keys(), root.threshold())?;
	if valid < root.threshold() + 2 {
		return Err(ModelError::new(
			RejectReason::CrossSignatureRequired,
			"migration requires the publisher and both home signatures",
		));
	}
	Ok(())
}

pub fn verify_recovery(signed: &SignedObject<Delegation>, root: &RootSet) -> Result<(), ModelError> {
	if !matches!(signed.payload, Delegation::Recovery(_)) {
		return Err(ModelError::new(RejectReason::WrongObjectKind, "expected a recovery event"));
	}
	let message = signed.signed_message(ObjectKind::Delegation);
	root.verify(&message, &signed.envelope)?;
	Ok(())
}
