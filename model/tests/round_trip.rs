use moraine_model::Canonical;
use moraine_model::artifact::Artifact;
use moraine_model::attestation::MirrorCommitment;
use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
use moraine_model::delegation::{KeyDelegation, Migration, RecoveryEvent, ReleaseWindow};
use moraine_model::event::{Event, EventKind};
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::profile::ProfileRevision;
use moraine_model::reference::ArtifactRef;
use moraine_model::release::ReleasePayload;

fn artifact(size: u64) -> Artifact {
	Artifact {
		digest: vec![7u8; 32],
		size,
		media_type: "application/java-archive".to_string(),
		filename: "example.jar".to_string(),
		is_primary: true,
		os_predicate: None,
		arch_predicate: None,
	}
}

fn release(size: u64) -> ReleasePayload {
	ReleasePayload {
		protocol: 1,
		project_id: "gd:sha256:aa".to_string(),
		game_id: "gd:sha256:bb".to_string(),
		release_nonce: vec![3u8; 16],
		human_version: "1.0.0".to_string(),
		channel: "release".to_string(),
		kind: "mod".to_string(),
		declared_time: 1_760_000_000,
		compatibility: vec![Compatibility {
			game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
			loader_id: None,
			loader_version_predicate: None,
			runtime_predicate: None,
			side: Side::Both,
			os_predicate: None,
			arch_predicate: None,
		}],
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

fn feed_entry(sequence: u64) -> FeedEntry {
	FeedEntry {
		protocol: 1,
		project_id: "gd:sha256:aa".to_string(),
		sequence,
		previous: (sequence > 1).then(|| vec![1u8; 32]),
		kind: "release-published".to_string(),
		object_digest: vec![2u8; 32],
		declared_at: 1_760_000_000,
	}
}

fn profile() -> ProfileRevision {
	ProfileRevision {
		protocol: 1,
		project_id: "gd:sha256:aa".to_string(),
		game_id: "gd:sha256:bb".to_string(),
		revision_nonce: vec![4u8; 16],
		display_name: "Example".to_string(),
		summary: "A summary".to_string(),
		description: "A description".to_string(),
		icon: None,
		gallery: Vec::new(),
		links: Vec::new(),
		communities: Vec::new(),
		categories: vec!["adventure".to_string()],
		tags: vec!["tag".to_string()],
		rights: None,
		declared_time: 1_760_000_000,
	}
}

fn genesis() -> Genesis {
	Genesis {
		protocol: 1,
		kind: GenesisKind::Project,
		nonce: vec![5u8; 16],
		roots: vec![RootKey::from_public_key(vec![9u8; 32]).expect("root")],
		threshold: 1,
		authorized_kinds: vec!["delegation".to_string(), "release".to_string(), "profile".to_string()],
		home_hint: None,
		contacts: None,
		created_at: 1_760_000_000,
	}
}

#[test]
fn an_artifact_survives_a_canonical_round_trip() {
	for size in [0u64, 1, 1024, i64::MAX as u64] {
		let original = artifact(size);
		let bytes = original.to_canonical_bytes();
		assert_eq!(
			Artifact::from_canonical_bytes(&bytes).expect("decode"),
			original,
			"size {size}"
		);
	}
}

#[test]
fn a_release_survives_a_canonical_round_trip() {
	let original = release(4096);
	let bytes = original.to_canonical_bytes();
	assert_eq!(ReleasePayload::from_canonical_bytes(&bytes).expect("decode"), original);
}

#[test]
fn a_feed_entry_survives_a_canonical_round_trip() {
	for sequence in [1u64, 2, 1000, i64::MAX as u64] {
		let original = feed_entry(sequence);
		let bytes = original.to_canonical_bytes();
		assert_eq!(
			FeedEntry::from_canonical_bytes(&bytes).expect("decode"),
			original,
			"seq {sequence}"
		);
	}
}

#[test]
fn a_profile_survives_a_canonical_round_trip() {
	let original = profile();
	let bytes = original.to_canonical_bytes();
	assert_eq!(ProfileRevision::from_canonical_bytes(&bytes).expect("decode"), original);
}

#[test]
fn a_genesis_survives_a_canonical_round_trip() {
	let original = genesis();
	let bytes = original.to_canonical_bytes();
	assert_eq!(Genesis::from_canonical_bytes(&bytes).expect("decode"), original);
}

#[test]
fn a_sequence_or_size_beyond_the_canonical_range_is_refused() {
	assert!(feed_entry(i64::MAX as u64 + 1).validate().is_err());
	assert!(release(i64::MAX as u64 + 1).validate().is_err());
	assert!(release(u64::MAX).validate().is_err());
}

#[test]
fn every_canonical_u64_field_refuses_a_wrapping_value() {
	let too_large = i64::MAX as u64 + 1;
	let root = RootKey::from_public_key(vec![9u8; 32]).expect("root");
	assert!(
		ArtifactRef {
			digest: vec![1u8; 32],
			size: too_large,
			media_type: "application/java-archive".to_string(),
		}
		.validate()
		.is_err()
	);
	assert!(
		MirrorCommitment {
			protocol: 1,
			mirror_id: "mirror".to_string(),
			artifact_digest: vec![1u8; 32],
			size: too_large,
			accepted_at: 1_760_000_000,
			retention_until: None,
			endpoint: "https://mirror.example".to_string(),
		}
		.validate()
		.is_err()
	);
	assert!(
		ReleaseWindow {
			from_seq: too_large,
			to_seq: too_large,
		}
		.validate()
		.is_err()
	);
	assert!(
		KeyDelegation {
			protocol: 1,
			project_id: "gd:sha256:aa".to_string(),
			delegate_key: root.clone(),
			allowed_kinds: vec!["release".to_string()],
			channels: None,
			max_version_scope: None,
			valid_from_seq: Some(too_large),
			expires_at: None,
			issued_at: 1_760_000_000,
			previous_delegation_digest: None,
		}
		.validate()
		.is_err()
	);
	assert!(
		Migration {
			protocol: 1,
			project_id: "gd:sha256:aa".to_string(),
			old_home: "https://old.example".to_string(),
			new_home: "https://new.example".to_string(),
			cutover_seq: too_large,
			reason: None,
			declared_time: 1_760_000_000,
		}
		.validate()
		.is_err()
	);
	assert!(
		RecoveryEvent {
			protocol: 1,
			project_id: "gd:sha256:aa".to_string(),
			compromised_key_ids: vec![root.key_id],
			valid_from_seq: too_large,
			replacement_roots: vec![root],
			affected_release_window: ReleaseWindow { from_seq: 1, to_seq: 2 },
			reason: "compromise".to_string(),
			declared_time: 1_760_000_000,
		}
		.validate()
		.is_err()
	);
	assert!(
		Event {
			protocol: 1,
			event_id: "event".to_string(),
			event_kind: EventKind::ForkDetected,
			project_id: "gd:sha256:aa".to_string(),
			game_id: None,
			feed_seq: Some(too_large),
			object_digest: None,
			advisory_digest: None,
			issued_at: 1_760_000_000,
		}
		.validate()
		.is_err()
	);
}
