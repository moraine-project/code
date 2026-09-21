use moraine_codec::Value;
use moraine_crypto::{ObjectKind, SigningKey};
use moraine_model::attestation::{Attestation, AttestationKind, AttestationObject, MirrorCommitment};
use moraine_model::changelog::{Changelog, ChangelogSection, LocaleSection};
use moraine_model::compatibility::Side;
use moraine_model::delegation::{Delegation, Migration, RecoveryEvent, ReleaseWindow};
use moraine_model::deny_list::{DenyList, DenyListEntry, DenyTarget};
use moraine_model::dependency::TargetKind;
use moraine_model::genesis::RootKey;
use moraine_model::location::{Location, LocationKind, LocationRecord};
use moraine_model::moderation::ScopeKind;
use moraine_model::modpack::{ModpackEntry, ModpackManifest};
use moraine_model::release::ReleaseObject;
use moraine_model::signed::sign_payload;

use crate::generate::{DECLARED_AT, Outcome, object_vector, raw_vector, sample_id, signer, trust};
use crate::vector::Vector;

fn root_of(key: &SigningKey) -> RootKey {
	RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).expect("root key")
}

fn build_migration(project_id: &str) -> Delegation {
	Delegation::Migration(Migration {
		protocol: 1,
		project_id: project_id.to_string(),
		old_home: "https://old.example.org".to_string(),
		new_home: "https://new.example.org".to_string(),
		cutover_seq: 42,
		reason: Some("datacenter move".to_string()),
		declared_time: DECLARED_AT,
	})
}

fn build_recovery(project_id: &str, compromised: &SigningKey, replacement: &SigningKey) -> Delegation {
	Delegation::Recovery(RecoveryEvent {
		protocol: 1,
		project_id: project_id.to_string(),
		compromised_key_ids: vec![compromised.key_id()],
		valid_from_seq: 10,
		replacement_roots: vec![root_of(replacement)],
		affected_release_window: ReleaseWindow { from_seq: 5, to_seq: 9 },
		reason: "release key compromise".to_string(),
		declared_time: DECLARED_AT,
	})
}

fn build_location() -> ReleaseObject {
	ReleaseObject::Location(LocationRecord {
		protocol: 1,
		artifact_digest: vec![0x33; 32],
		locations: vec![
			Location {
				url: "https://cdn.example.org/a.jar".to_string(),
				kind: LocationKind::Origin,
				operator_id: None,
			},
			Location {
				url: "https://mirror.example.net/a.jar".to_string(),
				kind: LocationKind::Mirror,
				operator_id: Some(sample_id("mirror-operator")),
			},
		],
		declared_time: DECLARED_AT,
	})
}

fn build_commitment() -> AttestationObject {
	AttestationObject::MirrorCommitment(MirrorCommitment {
		protocol: 1,
		mirror_id: sample_id("mirror-one"),
		artifact_digest: vec![0x33; 32],
		size: 4096,
		accepted_at: DECLARED_AT,
		retention_until: Some(DECLARED_AT + 90 * 86_400),
		endpoint: "https://mirror.example.net".to_string(),
	})
}

fn build_attestation() -> AttestationObject {
	AttestationObject::Evidence(Attestation {
		protocol: 1,
		artifact_digest: vec![0x33; 32],
		subject_kind: "release".to_string(),
		subject_id: sample_id("example-release"),
		kind: AttestationKind::BuildProvenance,
		media_type: "application/vnd.slsa.provenance+json".to_string(),
		body_digest: Some(vec![0x77; 32]),
		body_inline: None,
		signer_id: sample_id("builder-ci"),
		issued_at: DECLARED_AT,
	})
}

fn build_changelog(project_id: &str) -> Changelog {
	Changelog {
		protocol: 1,
		project_id: project_id.to_string(),
		release_id: None,
		locale_sections: vec![LocaleSection {
			locale: "en".to_string(),
			sections: vec![ChangelogSection {
				heading: "Fixes".to_string(),
				body: "Corrected a crash".to_string(),
				severity: Some("high".to_string()),
			}],
		}],
		declared_time: DECLARED_AT,
	}
}

fn build_modpack(project_id: &str) -> ModpackManifest {
	ModpackManifest {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: sample_id("minecraft"),
		loader_id: None,
		entries: vec![ModpackEntry {
			ordinal: 0,
			target_kind: TargetKind::Project,
			target_id: sample_id("included-project"),
			release_id: sample_id("included-release"),
			digest: vec![0xAB; 32],
			applies_to: Side::Both,
		}],
		overrides: Vec::new(),
		server_manifest_digest: None,
		declared_time: DECLARED_AT,
	}
}

fn build_deny_list(project_id: &str) -> DenyList {
	DenyList {
		protocol: 1,
		issuer_id: sample_id("directory"),
		entries: vec![DenyListEntry {
			target_kind: DenyTarget::Project,
			target_id: project_id.to_string(),
			reason_code: "malware-confirmed".to_string(),
			reason_taxonomy_version: 1,
			scope_kind: ScopeKind::Instance,
			scope_id: "directory.example".to_string(),
			valid_from: None,
			valid_until: None,
		}],
		issued_at: DECLARED_AT,
	}
}

pub(crate) fn vectors() -> Vec<Vector> {
	let k1 = signer(1);
	let k2 = signer(2);
	let k3 = signer(3);
	let project_id = sample_id("example-project");
	let mut vectors = Vec::with_capacity(16);

	let migration = build_migration(&project_id);
	let signed_migration = sign_payload(ObjectKind::Delegation, &migration, &[&k1, &k2, &k3]);
	let migration_trust = trust(&[&k1, &k2, &k3], 1);
	vectors.push(object_vector(
		"migration-cross-signed",
		"migration",
		ObjectKind::Delegation,
		&signed_migration,
		"accept",
		None,
		Some(migration_trust),
	));

	let signed_partial = sign_payload(ObjectKind::Delegation, &migration, &[&k1, &k2]);
	vectors.push(object_vector(
		"migration-missing-cross-signature",
		"migration",
		ObjectKind::Delegation,
		&signed_partial,
		"reject",
		Some("cross-signature-required"),
		Some(trust(&[&k1, &k2, &k3], 1)),
	));

	let recovery = build_recovery(&project_id, &k2, &k3);
	let signed_recovery = sign_payload(ObjectKind::Delegation, &recovery, &[&k1, &k2]);
	vectors.push(object_vector(
		"recovery-threshold-met",
		"recovery",
		ObjectKind::Delegation,
		&signed_recovery,
		"accept",
		None,
		Some(trust(&[&k1, &k2], 2)),
	));

	let signed_under = sign_payload(ObjectKind::Delegation, &recovery, &[&k1]);
	vectors.push(object_vector(
		"recovery-below-threshold",
		"recovery",
		ObjectKind::Delegation,
		&signed_under,
		"reject",
		Some("threshold-not-met"),
		Some(trust(&[&k1, &k2], 2)),
	));

	let location = build_location();
	let signed_location = sign_payload(ObjectKind::Release, &location, &[&k1]);
	vectors.push(object_vector(
		"location-record-valid",
		"locations",
		ObjectKind::Release,
		&signed_location,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let commitment = build_commitment();
	let signed_commitment = sign_payload(ObjectKind::Attestation, &commitment, &[&k1]);
	vectors.push(object_vector(
		"mirror-commitment-valid",
		"locations",
		ObjectKind::Attestation,
		&signed_commitment,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let evidence = build_attestation();
	let signed_evidence = sign_payload(ObjectKind::Attestation, &evidence, &[&k1]);
	vectors.push(object_vector(
		"attestation-build-provenance",
		"locations",
		ObjectKind::Attestation,
		&signed_evidence,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let changelog = build_changelog(&project_id);
	let signed_changelog = sign_payload(ObjectKind::Changelog, &changelog, &[&k1]);
	vectors.push(object_vector(
		"changelog-valid",
		"changelog",
		ObjectKind::Changelog,
		&signed_changelog,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let modpack = build_modpack(&project_id);
	let signed_modpack = sign_payload(ObjectKind::Modpack, &modpack, &[&k1]);
	vectors.push(object_vector(
		"modpack-valid",
		"modpack",
		ObjectKind::Modpack,
		&signed_modpack,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let deny_list = build_deny_list(&project_id);
	let signed_deny_list = sign_payload(ObjectKind::DenyList, &deny_list, &[&k1]);
	vectors.push(object_vector(
		"deny-list-valid",
		"deny-list",
		ObjectKind::DenyList,
		&signed_deny_list,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let empty_locations = Value::map([
		(Value::text("protocol"), Value::int(1)),
		(Value::text("type"), Value::text("location")),
		(Value::text("artifact_digest"), Value::bytes(vec![0x33; 32])),
		(Value::text("locations"), Value::array(Vec::new())),
		(Value::text("declared_time"), Value::int(DECLARED_AT)),
	]);
	vectors.push(raw_vector(
		"location-record-empty",
		"locations",
		ObjectKind::Release,
		empty_locations,
		&[&k1],
		Outcome {
			verdict: "reject",
			reason: Some("invalid-field-value"),
		},
		Some(trust(&[&k1], 1)),
	));

	let both_bodies = Value::map([
		(Value::text("protocol"), Value::int(1)),
		(Value::text("type"), Value::text("attestation")),
		(Value::text("artifact_digest"), Value::bytes(vec![0x33; 32])),
		(Value::text("subject_kind"), Value::text("release")),
		(Value::text("subject_id"), Value::text(sample_id("example-release"))),
		(Value::text("kind"), Value::text("sbom")),
		(Value::text("media_type"), Value::text("application/spdx+json")),
		(Value::text("body_digest"), Value::bytes(vec![0x77; 32])),
		(Value::text("body_inline"), Value::bytes(vec![0x01, 0x02])),
		(Value::text("signer_id"), Value::text(sample_id("builder-ci"))),
		(Value::text("issued_at"), Value::int(DECLARED_AT)),
	]);
	vectors.push(raw_vector(
		"attestation-both-bodies",
		"locations",
		ObjectKind::Attestation,
		both_bodies,
		&[&k1],
		Outcome {
			verdict: "reject",
			reason: Some("invalid-field-value"),
		},
		Some(trust(&[&k1], 1)),
	));

	vectors
}
