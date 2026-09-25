use moraine_crypto::{ObjectKind, object_id_string};

use crate::Canonical;
use crate::delegation::Delegation;
use crate::error::{ModelError, RejectReason};
use crate::genesis::{Genesis, GenesisKind};
use crate::signed::{SignedObject, TrustedKey, verify_envelope};

#[derive(Debug, Clone)]
pub struct RootSet {
	subject: String,
	keys: Vec<TrustedKey>,
	threshold: usize,
	authorized_kinds: Vec<String>,
	genesis_kind: GenesisKind,
}

impl RootSet {
	pub fn new(
		subject: &str,
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
		if subject.is_empty() {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"a root set must name the identity it establishes",
			));
		}
		genesis_kind.validate_authorized_kinds(&authorized_kinds)?;
		Ok(Self {
			subject: subject.to_string(),
			keys,
			threshold,
			authorized_kinds,
			genesis_kind,
		})
	}

	pub fn from_genesis(genesis: &Genesis, subject: &str) -> Result<Self, ModelError> {
		genesis.validate()?;
		if subject.is_empty() {
			return Err(ModelError::new(
				RejectReason::InvalidFieldValue,
				"a root set must name the identity it establishes",
			));
		}
		let expected = object_id_string(ObjectKind::Genesis, &genesis.to_canonical_bytes());
		if subject != expected {
			return Err(ModelError::new(
				RejectReason::WrongSubject,
				"the root set subject does not match the genesis identity",
			));
		}
		let keys = genesis
			.roots
			.iter()
			.map(|root| TrustedKey::new(&root.public_key))
			.collect::<Result<Vec<_>, _>>()?;
		Ok(Self {
			subject: subject.to_string(),
			keys,
			threshold: usize::try_from(genesis.threshold).unwrap_or(usize::MAX),
			authorized_kinds: genesis.authorized_kinds.clone(),
			genesis_kind: genesis.kind,
		})
	}

	pub fn subject(&self) -> &str {
		&self.subject
	}

	pub const fn threshold(&self) -> usize {
		self.threshold
	}

	pub const fn genesis_kind(&self) -> GenesisKind {
		self.genesis_kind
	}

	pub fn authorizes_kind(&self, kind: &str) -> bool {
		self.genesis_kind.allows_authorized_kind(kind) && self.authorized_kinds.iter().any(|authorized| authorized == kind)
	}

	pub fn keys(&self) -> &[TrustedKey] {
		&self.keys
	}

	pub fn verify(&self, message: &[u8], envelope: &crate::signed::SignatureEnvelope) -> Result<usize, ModelError> {
		verify_envelope(envelope, message, &self.keys, self.threshold)
	}
}

pub fn verify_genesis(signed: &SignedObject<Genesis>, subject: &str) -> Result<RootSet, ModelError> {
	let root = RootSet::from_genesis(&signed.payload, subject)?;
	let message = signed.signed_message(ObjectKind::Genesis);
	root.verify(&message, &signed.envelope)?;
	Ok(root)
}

pub fn verify_key_delegation(signed: &SignedObject<Delegation>, root: &RootSet) -> Result<(), ModelError> {
	let Delegation::Key(delegation) = &signed.payload else {
		return Err(ModelError::new(RejectReason::WrongObjectKind, "expected a key delegation"));
	};
	delegation.validate()?;
	if delegation.project_id != root.subject() {
		return Err(ModelError::new(
			RejectReason::WrongSubject,
			"the delegation names a different project than the root set",
		));
	}
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
	let Delegation::OwnershipTransfer(transfer) = &signed.payload else {
		return Err(ModelError::new(
			RejectReason::WrongObjectKind,
			"expected an ownership transfer",
		));
	};
	transfer.validate()?;
	if root.genesis_kind() != GenesisKind::Project {
		return Err(ModelError::new(
			RejectReason::UnauthorizedKind,
			"only a project root can authorize an ownership transfer",
		));
	}
	if transfer.project_id != root.subject() {
		return Err(ModelError::new(
			RejectReason::WrongSubject,
			"the transfer names a different project than the root set",
		));
	}
	if !root.authorizes_kind("delegation") {
		return Err(ModelError::new(
			RejectReason::UnauthorizedKind,
			"genesis does not authorize ownership transfers",
		));
	}
	let message = signed.signed_message(ObjectKind::Delegation);
	let valid = verify_envelope(&signed.envelope, &message, root.keys(), root.threshold())?;
	let signed_from = signed
		.envelope
		.signatures
		.iter()
		.any(|signature| signature.key_id == transfer.from_owner.key_id);
	let signed_to = signed
		.envelope
		.signatures
		.iter()
		.any(|signature| signature.key_id == transfer.to_owner.key_id);
	if valid < 2 || !signed_from || !signed_to {
		return Err(ModelError::new(
			RejectReason::TransferNeedsTwoSignatures,
			"ownership transfer requires the previous and new owner signatures",
		));
	}
	Ok(())
}

pub fn verify_migration(signed: &SignedObject<Delegation>, root: &RootSet) -> Result<(), ModelError> {
	let Delegation::Migration(migration) = &signed.payload else {
		return Err(ModelError::new(RejectReason::WrongObjectKind, "expected a migration record"));
	};
	if root.genesis_kind() != GenesisKind::Project {
		return Err(ModelError::new(
			RejectReason::UnauthorizedKind,
			"only a project root can authorize a migration",
		));
	}
	migration.validate()?;
	if migration.project_id != root.subject() {
		return Err(ModelError::new(
			RejectReason::WrongSubject,
			"the migration names a different project than the root set",
		));
	}
	if !root.authorizes_kind("delegation") {
		return Err(ModelError::new(
			RejectReason::UnauthorizedKind,
			"genesis does not authorize migrations",
		));
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
	let Delegation::Recovery(recovery) = &signed.payload else {
		return Err(ModelError::new(RejectReason::WrongObjectKind, "expected a recovery event"));
	};
	if root.genesis_kind() != GenesisKind::Project {
		return Err(ModelError::new(
			RejectReason::UnauthorizedKind,
			"only a project root can authorize a recovery event",
		));
	}
	recovery.validate()?;
	if recovery.project_id != root.subject() {
		return Err(ModelError::new(
			RejectReason::WrongSubject,
			"the recovery names a different project than the root set",
		));
	}
	if !root.authorizes_kind("delegation") {
		return Err(ModelError::new(
			RejectReason::UnauthorizedKind,
			"genesis does not authorize recovery events",
		));
	}
	let message = signed.signed_message(ObjectKind::Delegation);
	root.verify(&message, &signed.envelope)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use moraine_crypto::{ObjectKind, SigningKey};

	use super::*;
	use crate::delegation::{Delegation, OwnerRef, OwnershipTransfer};
	use crate::genesis::RootKey;
	use crate::signed::sign_payload;

	fn root_set(keys: &[&SigningKey]) -> RootSet {
		let public_keys = keys
			.iter()
			.map(|key| key.verifying_key().to_bytes().to_vec())
			.collect::<Vec<_>>();
		RootSet::new(
			"gd:sha256:project",
			&public_keys,
			2,
			vec!["delegation".to_string()],
			GenesisKind::Project,
		)
		.expect("root set")
	}

	fn transfer(project_id: &str, from: &SigningKey, to: &SigningKey) -> Delegation {
		Delegation::OwnershipTransfer(OwnershipTransfer {
			protocol: 1,
			project_id: project_id.to_string(),
			from_owner: OwnerRef {
				kind: "user".to_string(),
				id: "old".to_string(),
				key_id: from.key_id(),
			},
			to_owner: OwnerRef {
				kind: "user".to_string(),
				id: "new".to_string(),
				key_id: to.key_id(),
			},
			issued_at: 1_760_000_000,
			previous_delegation_digest: None,
		})
	}

	#[test]
	fn a_genesis_root_set_rejects_a_foreign_subject() {
		let key = SigningKey::from_seed(&[7; 32]);
		let genesis = Genesis {
			protocol: 1,
			kind: GenesisKind::Project,
			nonce: vec![0x11; 16],
			roots: vec![RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).expect("root")],
			threshold: 1,
			authorized_kinds: vec!["delegation".to_string()],
			home_hint: None,
			contacts: None,
			created_at: 1_760_000_000,
		};
		assert!(RootSet::from_genesis(&genesis, "gd:sha256:foreign").is_err());
	}

	#[test]
	fn ownership_transfer_binds_the_named_key_ids() {
		let root_a = SigningKey::from_seed(&[1; 32]);
		let root_b = SigningKey::from_seed(&[2; 32]);
		let stranger = SigningKey::from_seed(&[3; 32]);
		let root = root_set(&[&root_a, &root_b]);
		let mut payload = transfer(root.subject(), &root_a, &root_b);
		let Delegation::OwnershipTransfer(record) = &mut payload else {
			unreachable!()
		};
		record.from_owner.key_id = stranger.key_id();

		let mismatched = sign_payload(ObjectKind::Delegation, &payload, &[&root_a, &root_b]).expect("valid signed transfer");
		let error = verify_ownership_transfer(&mismatched, &root).expect_err("field binding");
		assert_eq!(error.reason, RejectReason::TransferNeedsTwoSignatures);

		let payload = transfer(root.subject(), &root_a, &root_b);
		let signed = sign_payload(ObjectKind::Delegation, &payload, &[&root_a, &root_b]).expect("valid signed transfer");
		assert!(verify_ownership_transfer(&signed, &root).is_ok());
	}

	#[test]
	fn ownership_transfer_rejects_a_foreign_project() {
		let root_a = SigningKey::from_seed(&[1; 32]);
		let root_b = SigningKey::from_seed(&[2; 32]);
		let root = root_set(&[&root_a, &root_b]);
		let payload = transfer("gd:sha256:other", &root_a, &root_b);
		let signed = sign_payload(ObjectKind::Delegation, &payload, &[&root_a, &root_b]).expect("valid signed transfer");

		let error = verify_ownership_transfer(&signed, &root).expect_err("foreign project");
		assert_eq!(error.reason, RejectReason::WrongSubject);
	}
}
