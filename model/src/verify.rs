use std::fmt;

use moraine_codec::Value;
use moraine_crypto::{ObjectKind, object_id};

use crate::attestation::AttestationObject;
use crate::canonical::Canonical;
use crate::changelog::Changelog;
use crate::definition::{GameDef, LoaderObject, RuntimeDef};
use crate::delegation::{Delegation, DelegationPurpose};
use crate::error::{ModelError, RejectReason};
use crate::feed::FeedEntry;
use crate::genesis::{Genesis, GenesisKind};
use crate::modpack::ModpackManifest;
use crate::profile::ProfileRevision;
use crate::release::ReleaseObject;
use crate::signed::{ObjectPayload, SignedObject, TrustedKey, verify_envelope};
use crate::trust::{
	RootSet, verify_genesis as verify_genesis_signed, verify_key_delegation, verify_migration, verify_ownership_transfer,
	verify_recovery,
};

#[derive(Debug)]
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
	UnauthorizedKind(ObjectKind),
}

impl fmt::Display for VerifyError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Decode(error) => write!(f, "invalid object: {error}"),
			Self::Signature(error) => write!(f, "signature rejected: {error}"),
			Self::UnsupportedKind(kind) => write!(f, "object kind `{}` is not accepted yet", kind.as_str()),
			Self::UnauthorizedKind(kind) => write!(f, "the genesis does not authorize object kind `{}`", kind.as_str()),
		}
	}
}

impl std::error::Error for VerifyError {}

pub fn verify_genesis(wire: &[u8]) -> Result<(RootSet, VerifiedObject), VerifyError> {
	let signed = SignedObject::<Genesis>::from_bytes(wire).map_err(VerifyError::Decode)?;
	let root = verify_genesis_signed(&signed, &signed.id(ObjectKind::Genesis)).map_err(VerifyError::Signature)?;
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
		ObjectKind::Delegation => verify_delegation(wire, root),
		ObjectKind::Release => verify_typed::<ReleaseObject>(kind, wire, root),
		ObjectKind::Profile => verify_typed::<ProfileRevision>(kind, wire, root),
		ObjectKind::FeedEntry => verify_typed::<FeedEntry>(kind, wire, root),
		ObjectKind::Attestation => verify_typed::<AttestationObject>(kind, wire, root),
		ObjectKind::GameDef => verify_typed::<GameDef>(kind, wire, root),
		ObjectKind::LoaderDef => verify_typed::<LoaderObject>(kind, wire, root),
		ObjectKind::RuntimeDef => verify_typed::<RuntimeDef>(kind, wire, root),
		ObjectKind::Modpack => verify_typed::<ModpackManifest>(kind, wire, root),
		ObjectKind::Changelog => verify_typed::<Changelog>(kind, wire, root),
		ObjectKind::Advisory | ObjectKind::DenyList => Err(VerifyError::UnsupportedKind(kind)),
		other => Err(VerifyError::UnsupportedKind(other)),
	}
}

pub fn verify_object_authorized(
	kind: ObjectKind,
	wire: &[u8],
	root: &RootSet,
	delegations: &[SignedObject<Delegation>],
	now: i64,
) -> Result<VerifiedObject, VerifyError> {
	match verify_object(kind, wire, root) {
		Ok(object) => Ok(object),
		Err(VerifyError::Signature(error)) => match kind {
			ObjectKind::Delegation => Err(VerifyError::Signature(error)),
			ObjectKind::Release => verify_delegated::<ReleaseObject>(kind, wire, root, delegations, now, error),
			ObjectKind::Profile => verify_delegated::<ProfileRevision>(kind, wire, root, delegations, now, error),
			ObjectKind::FeedEntry => verify_delegated::<FeedEntry>(kind, wire, root, delegations, now, error),
			ObjectKind::Attestation => verify_delegated::<AttestationObject>(kind, wire, root, delegations, now, error),
			ObjectKind::GameDef => verify_delegated::<GameDef>(kind, wire, root, delegations, now, error),
			ObjectKind::LoaderDef => verify_delegated::<LoaderObject>(kind, wire, root, delegations, now, error),
			ObjectKind::RuntimeDef => verify_delegated::<RuntimeDef>(kind, wire, root, delegations, now, error),
			ObjectKind::Modpack => verify_delegated::<ModpackManifest>(kind, wire, root, delegations, now, error),
			ObjectKind::Changelog => verify_delegated::<Changelog>(kind, wire, root, delegations, now, error),
			other => Err(VerifyError::UnsupportedKind(other)),
		},
		Err(other) => Err(other),
	}
}

fn verify_delegation(wire: &[u8], root: &RootSet) -> Result<VerifiedObject, VerifyError> {
	let kind = ObjectKind::Delegation;
	if !root.authorizes_kind(kind.as_str()) {
		return Err(VerifyError::UnauthorizedKind(kind));
	}
	let signed = SignedObject::<Delegation>::from_bytes(wire).map_err(VerifyError::Decode)?;
	match signed.payload.purpose() {
		DelegationPurpose::Key => verify_key_delegation(&signed, root),
		DelegationPurpose::OwnershipTransfer => verify_ownership_transfer(&signed, root),
		DelegationPurpose::Migration => verify_migration(&signed, root),
		DelegationPurpose::Recovery => verify_recovery(&signed, root),
	}
	.map_err(VerifyError::Signature)?;
	bind_subject(kind, &signed.payload.to_value(), root)?;
	Ok(VerifiedObject {
		kind,
		digest: object_id(kind, &signed.payload_bytes),
		id: signed.id(kind),
		payload_bytes: signed.payload_bytes.clone(),
		wire_bytes: wire.to_vec(),
	})
}

fn verify_delegated<T: ObjectPayload>(
	kind: ObjectKind,
	wire: &[u8],
	root: &RootSet,
	delegations: &[SignedObject<Delegation>],
	now: i64,
	root_error: ModelError,
) -> Result<VerifiedObject, VerifyError> {
	if T::KIND != kind {
		return Err(VerifyError::Signature(ModelError::new(
			RejectReason::WrongObjectKind,
			format!("payload belongs to `{}`, not `{}`", T::KIND.as_str(), kind.as_str()),
		)));
	}
	let signed = SignedObject::<T>::from_bytes(wire).map_err(VerifyError::Decode)?;
	let message = signed.signed_message(kind);
	for candidate in delegations {
		if verify_key_delegation(candidate, root).is_err() {
			continue;
		}
		let Delegation::Key(delegation) = &candidate.payload else {
			continue;
		};
		if !delegation.allowed_kinds.iter().any(|allowed| allowed == kind.as_str()) {
			continue;
		}
		if delegation.expires_at.is_some_and(|expires| expires <= now) {
			continue;
		}
		if delegation.channels.is_some() || delegation.max_version_scope.is_some() || delegation.valid_from_seq.is_some() {
			continue;
		}
		let trusted = TrustedKey::new(&delegation.delegate_key.public_key).map_err(VerifyError::Signature)?;
		if verify_envelope(&signed.envelope, &message, std::slice::from_ref(&trusted), 1).is_ok() {
			bind_subject(kind, &signed.payload.to_value(), root)?;
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

fn bind_subject(kind: ObjectKind, payload: &Value, root: &RootSet) -> Result<(), VerifyError> {
	let declared = |field: &str| {
		let Value::Map(pairs) = payload else { return None };
		pairs
			.iter()
			.find(|(key, _)| matches!(key, Value::Text(name) if name == field))
			.and_then(|(_, value)| value.as_text())
	};
	let require = |field: &str, expected: &str| -> Result<(), VerifyError> {
		match declared(field) {
			Some(value) if value == expected => Ok(()),
			Some(value) => Err(VerifyError::Signature(ModelError::new(
				RejectReason::WrongSubject,
				format!("the object claims `{value}` but the root set establishes `{expected}`"),
			))),
			None => Err(VerifyError::Decode(ModelError::new(
				RejectReason::WrongSubject,
				format!("the object does not identify its `{field}` subject"),
			))),
		}
	};
	match kind {
		ObjectKind::GameDef => {
			if root.genesis_kind() != GenesisKind::Game {
				return Err(VerifyError::UnauthorizedKind(kind));
			}
			require("game_id", root.subject())?;
		}
		ObjectKind::LoaderDef => {
			if root.genesis_kind() != GenesisKind::Loader {
				return Err(VerifyError::UnauthorizedKind(kind));
			}
			let field = if declared("type") == Some("mapping") {
				"accepting_loader_id"
			} else {
				"loader_id"
			};
			require(field, root.subject())?;
		}
		ObjectKind::RuntimeDef => {
			if root.genesis_kind() != GenesisKind::Runtime {
				return Err(VerifyError::UnauthorizedKind(kind));
			}
			require("runtime_id", root.subject())?;
		}
		ObjectKind::Attestation => {
			if declared("subject_kind") == Some("project") {
				require("subject_id", root.subject())?;
			}
			require("signer_id", root.subject())?;
		}
		ObjectKind::Release if matches!(declared("type"), Some("release") | Some("withdrawal")) => {
			require("project_id", root.subject())?;
		}
		ObjectKind::Delegation
		| ObjectKind::Profile
		| ObjectKind::FeedEntry
		| ObjectKind::Changelog
		| ObjectKind::Modpack => require("project_id", root.subject())?,
		_ => {}
	}
	Ok(())
}

fn verify_typed<T: ObjectPayload>(kind: ObjectKind, wire: &[u8], root: &RootSet) -> Result<VerifiedObject, VerifyError> {
	if T::KIND != kind {
		return Err(VerifyError::Signature(ModelError::new(
			RejectReason::WrongObjectKind,
			format!("payload belongs to `{}`, not `{}`", T::KIND.as_str(), kind.as_str()),
		)));
	}
	if kind != ObjectKind::FeedEntry && !root.authorizes_kind(kind.as_str()) {
		return Err(VerifyError::UnauthorizedKind(kind));
	}
	let signed = SignedObject::<T>::from_bytes(wire).map_err(VerifyError::Decode)?;
	signed
		.verify_threshold(kind, root.keys(), root.threshold())
		.map_err(VerifyError::Signature)?;
	bind_subject(kind, &signed.payload.to_value(), root)?;
	Ok(VerifiedObject {
		kind,
		digest: object_id(kind, &signed.payload_bytes),
		id: signed.id(kind),
		payload_bytes: signed.payload_bytes.clone(),
		wire_bytes: wire.to_vec(),
	})
}

#[cfg(test)]
mod tests {
	use moraine_crypto::{ObjectKind, SigningKey};

	use super::*;
	use crate::artifact::Artifact;
	use crate::attestation::{Attestation, AttestationKind, AttestationObject};
	use crate::compatibility::{Compatibility, Predicate, Scheme, Side};
	use crate::delegation::{KeyDelegation, Migration, OwnerRef, OwnershipTransfer};
	use crate::genesis::{Genesis, GenesisKind, RootKey};
	use crate::release::{ReleasePayload, Withdrawal};
	use crate::signed::sign_payload;
	use crate::trust::RootSet;

	fn root_set(key: &SigningKey) -> RootSet {
		let genesis = Genesis {
			protocol: 1,
			kind: GenesisKind::Project,
			nonce: vec![0x11; 16],
			roots: vec![RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).expect("root")],
			threshold: 1,
			authorized_kinds: vec![
				"delegation".to_string(),
				"release".to_string(),
				"feed-entry".to_string(),
				"profile".to_string(),
				"attestation".to_string(),
			],
			home_hint: None,
			contacts: None,
			created_at: 1_760_000_000,
		};
		let signed = sign_payload(ObjectKind::Genesis, &genesis, &[key]).expect("valid signed genesis");
		verify_genesis(&signed.wire_bytes()).expect("genesis").0
	}

	fn release_wire(project_id: &str, signer: &SigningKey) -> Vec<u8> {
		let release = ReleasePayload {
			protocol: 1,
			project_id: project_id.to_string(),
			game_id: "gd:sha256:game".to_string(),
			release_nonce: vec![0x22; 16],
			human_version: "1.0.0".to_string(),
			channel: "release".to_string(),
			kind: "mod".to_string(),
			declared_time: 1_760_000_000,
			compatibility: vec![Compatibility {
				game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
				loader_id: None,
				loader_version_predicate: None,
				side: Side::Both,
				runtime_predicate: None,
				os_predicate: None,
				arch_predicate: None,
			}],
			artifacts: vec![Artifact {
				digest: vec![0xAB; 32],
				size: 10,
				media_type: "application/java-archive".to_string(),
				filename: "example.jar".to_string(),
				is_primary: true,
				os_predicate: None,
				arch_predicate: None,
			}],
			dependencies: Vec::new(),
			source_reference: None,
			changelog_digest: None,
			license_expression: None,
			rights: None,
			sbom_digest: None,
			minimum_verifier_version: 1,
			critical_extensions: Vec::new(),
		};
		sign_payload(ObjectKind::Release, &release, &[signer])
			.expect("valid signed release")
			.wire_bytes()
	}

	fn withdrawal(project_id: &str) -> Withdrawal {
		Withdrawal {
			protocol: 1,
			project_id: project_id.to_string(),
			release_id: "gd:sha256:release".to_string(),
			reason: "broken".to_string(),
			note: None,
			declared_time: 1_760_000_000,
		}
	}

	fn withdrawal_wire(project_id: &str, signer: &SigningKey) -> Vec<u8> {
		sign_payload(ObjectKind::Release, &withdrawal(project_id), &[signer])
			.expect("valid signed withdrawal")
			.wire_bytes()
	}

	fn delegation(
		root: &RootSet,
		root_key: &SigningKey,
		delegate: &SigningKey,
		restricted: bool,
	) -> SignedObject<Delegation> {
		let payload = KeyDelegation {
			protocol: 1,
			project_id: root.subject().to_string(),
			delegate_key: RootKey::from_public_key(delegate.verifying_key().to_bytes().to_vec()).expect("delegate"),
			allowed_kinds: vec!["release".to_string()],
			channels: restricted.then(|| vec!["beta".to_string()]),
			max_version_scope: None,
			valid_from_seq: None,
			expires_at: None,
			issued_at: 1_760_000_000,
			previous_delegation_digest: None,
		};
		sign_payload(ObjectKind::Delegation, &Delegation::Key(payload), &[root_key]).expect("valid signed delegation")
	}

	fn assert_delegation_rejected_by_root_rules(wire: &[u8], root: &RootSet, reason: RejectReason) {
		for result in [
			verify_object(ObjectKind::Delegation, wire, root),
			verify_object_authorized(ObjectKind::Delegation, wire, root, &[], 1_770_000_000),
		] {
			assert!(
				matches!(result, Err(VerifyError::Signature(ref error)) if error.reason == reason),
				"{result:?}"
			);
		}
	}

	#[test]
	fn honors_an_unrestricted_delegation() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let delegate_key = SigningKey::from_seed(&[2; 32]);
		let root = root_set(&root_key);
		let wire = release_wire(root.subject(), &delegate_key);
		let allowed = vec![delegation(&root, &root_key, &delegate_key, false)];

		let result = verify_object_authorized(ObjectKind::Release, &wire, &root, &allowed, 1_770_000_000);
		assert!(result.is_ok(), "{:?}", result.err());
	}

	#[test]
	fn refuses_a_delegation_it_cannot_scope() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let delegate_key = SigningKey::from_seed(&[2; 32]);
		let root = root_set(&root_key);
		let wire = release_wire(root.subject(), &delegate_key);
		let restricted = vec![delegation(&root, &root_key, &delegate_key, true)];

		assert!(verify_object_authorized(ObjectKind::Release, &wire, &root, &restricted, 1_770_000_000).is_err());
	}

	#[test]
	fn refuses_a_delegation_that_has_expired() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let delegate_key = SigningKey::from_seed(&[2; 32]);
		let root = root_set(&root_key);
		let wire = release_wire(root.subject(), &delegate_key);
		let expired = delegation(&root, &root_key, &delegate_key, false).wire_bytes();
		let mut expired = SignedObject::<Delegation>::from_bytes(&expired).expect("delegation");
		let Delegation::Key(payload) = &mut expired.payload else {
			unreachable!()
		};
		payload.expires_at = Some(1_750_000_000);
		let expired = sign_payload(ObjectKind::Delegation, &expired.payload, &[&root_key]).expect("valid signed delegation");

		assert!(verify_object_authorized(ObjectKind::Release, &wire, &root, &[expired], 1_770_000_000).is_err());
	}

	#[test]
	fn a_root_refuses_an_object_that_names_another_project() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let root = root_set(&root_key);
		let wire = release_wire("gd:sha256:someone-else", &root_key);

		let error = verify_object(ObjectKind::Release, &wire, &root).expect_err("a root must not sign for another project");
		assert!(error.to_string().contains("gd:sha256:someone-else"), "{error}");
		assert!(error.to_string().contains(root.subject()), "{error}");
	}

	#[test]
	fn a_withdrawal_is_bound_to_one_project() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let root = root_set(&root_key);

		assert!(verify_object(ObjectKind::Release, &withdrawal_wire(root.subject(), &root_key), &root).is_ok());
		assert!(sign_payload(ObjectKind::Release, &withdrawal(""), &[&root_key]).is_err());

		let foreign = withdrawal_wire("gd:sha256:someone-else", &root_key);
		let error = verify_object(ObjectKind::Release, &foreign, &root)
			.expect_err("a root must not withdraw a release for another project");
		assert!(
			matches!(error, VerifyError::Signature(ref error) if error.reason == RejectReason::WrongSubject),
			"{error:?}"
		);
	}

	#[test]
	fn a_delegation_for_another_project_grants_nothing() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let delegate_key = SigningKey::from_seed(&[2; 32]);
		let root = root_set(&root_key);
		let wire = release_wire(root.subject(), &delegate_key);
		let mut foreign = delegation(&root, &root_key, &delegate_key, false);
		let Delegation::Key(payload) = &mut foreign.payload else {
			unreachable!()
		};
		payload.project_id = "gd:sha256:someone-else".to_string();
		let foreign = sign_payload(ObjectKind::Delegation, &foreign.payload, &[&root_key]).expect("valid signed delegation");

		assert!(verify_object_authorized(ObjectKind::Release, &wire, &root, &[foreign], 1_770_000_000).is_err());
	}

	#[test]
	fn a_delegation_that_was_never_root_signed_grants_nothing() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let delegate_key = SigningKey::from_seed(&[2; 32]);
		let forger = SigningKey::from_seed(&[3; 32]);
		let root = root_set(&root_key);
		let wire = release_wire(root.subject(), &delegate_key);
		let forged = delegation(&root, &forger, &delegate_key, false);

		assert!(verify_object_authorized(ObjectKind::Release, &wire, &root, &[forged], 1_770_000_000).is_err());
	}

	#[test]
	fn a_delegation_cannot_grant_a_kind_the_root_withholds() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let delegate_key = SigningKey::from_seed(&[2; 32]);
		let root = root_set(&root_key);
		let wire = release_wire(root.subject(), &delegate_key);
		let mut overreach = delegation(&root, &root_key, &delegate_key, false);
		let Delegation::Key(payload) = &mut overreach.payload else {
			unreachable!()
		};
		payload.allowed_kinds = vec!["deny-list".to_string()];
		let overreach =
			sign_payload(ObjectKind::Delegation, &overreach.payload, &[&root_key]).expect("valid signed delegation");

		assert!(verify_object_authorized(ObjectKind::Release, &wire, &root, &[overreach], 1_770_000_000).is_err());
	}

	#[test]
	fn a_delegation_does_not_authorize_another_kind() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let delegate_key = SigningKey::from_seed(&[2; 32]);
		let root = root_set(&root_key);
		let wire = release_wire(root.subject(), &delegate_key);
		let profile_only =
			SignedObject::<Delegation>::from_bytes(&delegation(&root, &root_key, &delegate_key, false).wire_bytes())
				.expect("delegation");
		let mut profile_only = profile_only.payload;
		let Delegation::Key(payload) = &mut profile_only else {
			unreachable!()
		};
		payload.allowed_kinds = vec!["profile".to_string()];
		let profile_only =
			sign_payload(ObjectKind::Delegation, &profile_only, &[&root_key]).expect("valid signed delegation");

		assert!(verify_object_authorized(ObjectKind::Release, &wire, &root, &[profile_only], 1_770_000_000).is_err());
	}

	#[test]
	fn one_sided_ownership_transfer_is_rejected() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let new_owner_key = SigningKey::from_seed(&[2; 32]);
		let root = root_set(&root_key);
		let transfer = Delegation::OwnershipTransfer(OwnershipTransfer {
			protocol: 1,
			project_id: root.subject().to_string(),
			from_owner: OwnerRef {
				kind: "user".to_string(),
				id: "old-owner".to_string(),
				key_id: root_key.key_id(),
			},
			to_owner: OwnerRef {
				kind: "user".to_string(),
				id: "new-owner".to_string(),
				key_id: new_owner_key.key_id(),
			},
			issued_at: 1_760_000_000,
			previous_delegation_digest: None,
		});
		let wire = sign_payload(ObjectKind::Delegation, &transfer, &[&root_key])
			.expect("valid signed transfer")
			.wire_bytes();

		assert_delegation_rejected_by_root_rules(&wire, &root, RejectReason::TransferNeedsTwoSignatures);
	}

	#[test]
	fn under_threshold_migration_is_rejected() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let root = root_set(&root_key);
		let migration = Delegation::Migration(Migration {
			protocol: 1,
			project_id: root.subject().to_string(),
			old_home: "https://old.example".to_string(),
			new_home: "https://new.example".to_string(),
			cutover_seq: 1,
			reason: None,
			declared_time: 1_760_000_000,
		});
		let wire = sign_payload(ObjectKind::Delegation, &migration, &[&root_key])
			.expect("valid signed migration")
			.wire_bytes();

		assert_delegation_rejected_by_root_rules(&wire, &root, RejectReason::CrossSignatureRequired);
	}

	#[test]
	fn delegated_keys_cannot_authorize_delegations() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let delegate_key = SigningKey::from_seed(&[2; 32]);
		let root = root_set(&root_key);
		let mut grant = delegation(&root, &root_key, &delegate_key, false).payload;
		let Delegation::Key(payload) = &mut grant else {
			unreachable!()
		};
		payload.allowed_kinds = vec!["delegation".to_string()];
		let grant = sign_payload(ObjectKind::Delegation, &grant, &[&root_key]).expect("valid signed delegation");
		let wire = sign_payload(ObjectKind::Delegation, &grant.payload, &[&delegate_key])
			.expect("valid signed delegation")
			.wire_bytes();

		assert!(verify_object_authorized(ObjectKind::Delegation, &wire, &root, &[grant], 1_770_000_000).is_err());
	}

	#[test]
	fn a_project_root_cannot_impersonate_an_attestation_provider() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let root = root_set(&root_key);
		let payload = AttestationObject::Evidence(Attestation {
			protocol: 1,
			artifact_digest: vec![0xAB; 32],
			subject_kind: "project".to_string(),
			subject_id: root.subject().to_string(),
			kind: AttestationKind::BuildProvenance,
			media_type: "application/json".to_string(),
			body_digest: None,
			body_inline: None,
			signer_id: "provider".to_string(),
			issued_at: 1_760_000_000,
		});
		let signed = sign_payload(ObjectKind::Attestation, &payload, &[&root_key]).expect("valid signed attestation");
		assert!(verify_object(ObjectKind::Attestation, &signed.wire_bytes(), &root).is_err());
	}

	#[test]
	fn project_roots_do_not_verify_external_authorities() {
		let root_key = SigningKey::from_seed(&[1; 32]);
		let root = root_set(&root_key);
		for kind in [ObjectKind::Advisory, ObjectKind::DenyList] {
			assert!(matches!(verify_object(kind, &[], &root), Err(VerifyError::UnsupportedKind(k)) if k == kind));
		}
	}
}
