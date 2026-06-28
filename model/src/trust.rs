use moraine_crypto::ObjectKind;

use crate::delegation::{Delegation, DelegationPurpose};
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
	if signed.payload.purpose != DelegationPurpose::Key {
		return Err(ModelError::new(RejectReason::WrongObjectKind, "expected a key delegation"));
	}
	if !root.authorizes_kind("delegation") {
		return Err(ModelError::new(
			RejectReason::UnauthorizedKind,
			"genesis does not authorize delegations",
		));
	}
	let allowed = signed
		.payload
		.allowed_kinds
		.as_ref()
		.expect("validated key delegation has allowed_kinds");
	for kind in allowed {
		if !root.authorizes_kind(kind) {
			return Err(ModelError::new(
				RejectReason::UnauthorizedKind,
				format!("delegation grants `{kind}`, which genesis does not authorize"),
			));
		}
	}
	let delegate = signed
		.payload
		.delegate_key
		.as_ref()
		.expect("validated key delegation has a delegate key");
	let derived = moraine_crypto::key_id(moraine_crypto::ALG_ED25519, &delegate.public_key)
		.map_err(|_| ModelError::new(RejectReason::InvalidFieldValue, "invalid delegate public key"))?;
	if derived != delegate.key_id {
		return Err(ModelError::new(
			RejectReason::InvalidFieldValue,
			"delegate key_id does not match its public key",
		));
	}
	let message = signed.signed_message(ObjectKind::Delegation);
	root.verify(&message, &signed.envelope)?;
	Ok(())
}

pub fn verify_ownership_transfer(signed: &SignedObject<Delegation>, root: &RootSet) -> Result<(), ModelError> {
	if signed.payload.purpose != DelegationPurpose::OwnershipTransfer {
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
