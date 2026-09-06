use moraine_crypto::{ObjectKind as Kind, SigningKey, object_id};
use moraine_model::artifact::Artifact;
use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
use moraine_model::definition::{GameDef, LoaderDef, LoaderObject, LoaderRelease, VersionSyntax};
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::release::ReleasePayload;
use moraine_model::signed::sign_payload;

pub(crate) fn key(byte: u8) -> SigningKey {
	SigningKey::from_seed(&[byte; 32])
}

pub(crate) fn sample_id(label: &str) -> String {
	format!(
		"gd:sha256:{}",
		hex::encode(moraine_crypto::object_id(Kind::Release, label.as_bytes()))
	)
}

pub(crate) const PROJECT_KINDS: &[&str] = &["delegation", "release", "profile"];

pub(crate) fn game_genesis_wire(key: &SigningKey) -> Vec<u8> {
	let genesis = Genesis {
		protocol: 1,
		kind: GenesisKind::Game,
		nonce: vec![0x21; 16],
		roots: vec![RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).expect("root")],
		threshold: 1,
		authorized_kinds: vec!["delegation".to_string(), "game-def".to_string()],
		home_hint: None,
		contacts: None,
		created_at: 1_760_000_000,
	};
	sign_payload(Kind::Genesis, &genesis, &[key]).wire_bytes()
}

pub(crate) fn game_definition_wire(key: &SigningKey, game_id: &str) -> Vec<u8> {
	let definition = GameDef {
		protocol: 1,
		game_id: game_id.to_string(),
		display_name: "Minecraft".to_string(),
		version_syntax: VersionSyntax {
			kind: "semver".to_string(),
			pattern: None,
		},
		version_ordering: "semver".to_string(),
		loaders_allowed: true,
		loader_authorities: Vec::new(),
		categories: Vec::new(),
		tags: Vec::new(),
		metadata_extractor: None,
		install_adapter: None,
		declared_time: 1_760_000_000,
	};
	sign_payload(Kind::GameDef, &definition, &[key]).wire_bytes()
}

pub(crate) fn runtime_genesis_wire(key: &SigningKey) -> Vec<u8> {
	let genesis = Genesis {
		protocol: 1,
		kind: GenesisKind::Runtime,
		nonce: vec![0x72; 16],
		roots: vec![RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).expect("root")],
		threshold: 1,
		authorized_kinds: vec!["delegation".to_string(), "runtime-def".to_string()],
		home_hint: None,
		contacts: None,
		created_at: 1_760_000_000,
	};
	sign_payload(Kind::Genesis, &genesis, &[key]).wire_bytes()
}

pub(crate) fn runtime_definition_wire(key: &SigningKey, runtime_id: &str) -> Vec<u8> {
	let definition = moraine_model::definition::RuntimeDef {
		protocol: 1,
		runtime_id: runtime_id.to_string(),
		kind: "java".to_string(),
		display_name: "Java".to_string(),
		version_ordering: "semver".to_string(),
		declared_time: 1_760_000_000,
	};
	sign_payload(Kind::RuntimeDef, &definition, &[key]).wire_bytes()
}

pub(crate) fn loader_genesis_wire(key: &SigningKey) -> Vec<u8> {
	let genesis = Genesis {
		protocol: 1,
		kind: GenesisKind::Loader,
		nonce: vec![0x71; 16],
		roots: vec![RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).expect("root")],
		threshold: 1,
		authorized_kinds: vec!["delegation".to_string(), "loader-def".to_string()],
		home_hint: None,
		contacts: None,
		created_at: 1_760_000_000,
	};
	sign_payload(Kind::Genesis, &genesis, &[key]).wire_bytes()
}

pub(crate) fn loader_definition_wire(key: &SigningKey, loader_id: &str, game_id: &str) -> Vec<u8> {
	sign_payload(
		Kind::LoaderDef,
		&LoaderObject::Definition(LoaderDef {
			protocol: 1,
			loader_id: loader_id.to_string(),
			game_id: game_id.to_string(),
			display_name: "Fabric".to_string(),
			version_ordering: "semver".to_string(),
			bootstrap: None,
			accepted_artifacts: None,
			declared_time: 1_760_000_000,
		}),
		&[key],
	)
	.wire_bytes()
}

pub(crate) fn loader_release_wire(key: &SigningKey, loader_id: &str, version: &str) -> Vec<u8> {
	sign_payload(
		Kind::LoaderDef,
		&LoaderObject::Release(LoaderRelease {
			protocol: 1,
			loader_id: loader_id.to_string(),
			version_id: version.to_string(),
			game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
			runtime_id: None,
			runtime_predicate: None,
			bootstrap: None,
			declared_time: 1_760_000_000,
		}),
		&[key],
	)
	.wire_bytes()
}

pub(crate) fn loader_acceptance_wire(
	key: &SigningKey,
	accepting_loader_id: &str,
	accepted_loader_id: &str,
	game_id: &str,
	declared_time: i64,
) -> Vec<u8> {
	use moraine_model::definition::{DeclaredBy, LoaderAcceptance, Qualification};
	sign_payload(
		Kind::LoaderDef,
		&LoaderObject::Acceptance(LoaderAcceptance {
			protocol: 1,
			accepting_loader_id: accepting_loader_id.to_string(),
			accepted_loader_id: accepted_loader_id.to_string(),
			game_id: game_id.to_string(),
			game_version_predicate: None,
			loader_version_predicate: None,
			accepted_version_predicate: None,
			qualification: Qualification::Native,
			declared_by: DeclaredBy {
				kind: "loader-authority".to_string(),
				id: accepting_loader_id.to_string(),
			},
			evidence_digest: None,
			declared_time,
		}),
		&[key],
	)
	.wire_bytes()
}

pub(crate) fn genesis_wire(signer: &SigningKey, kinds: &[&str]) -> Vec<u8> {
	genesis_wire_roots(&[signer], kinds)
}

pub(crate) fn genesis_wire_roots(roots: &[&SigningKey], kinds: &[&str]) -> Vec<u8> {
	let genesis = Genesis {
		protocol: 1,
		kind: GenesisKind::Project,
		nonce: vec![0x11; 16],
		roots: roots
			.iter()
			.map(|signer| RootKey::from_public_key(signer.verifying_key().to_bytes().to_vec()).expect("root"))
			.collect(),
		threshold: 1,
		authorized_kinds: kinds.iter().map(|kind| kind.to_string()).collect(),
		home_hint: None,
		contacts: None,
		created_at: 1_760_000_000,
	};
	sign_payload(Kind::Genesis, &genesis, roots.to_vec().as_slice()).wire_bytes()
}

pub(crate) fn release_wire(signer: &SigningKey, project_id: &str) -> (Vec<u8>, [u8; 32]) {
	release_wire_variant(signer, project_id, 0x42, "1.0.0")
}

pub(crate) fn release_wire_variant(
	signer: &SigningKey,
	project_id: &str,
	nonce: u8,
	human_version: &str,
) -> (Vec<u8>, [u8; 32]) {
	release_wire_for_game(signer, project_id, nonce, human_version, "1.20.1")
}

pub(crate) fn release_wire_for_game(
	signer: &SigningKey,
	project_id: &str,
	nonce: u8,
	human_version: &str,
	game_version: &str,
) -> (Vec<u8>, [u8; 32]) {
	release_wire_for_game_with_loader(signer, project_id, nonce, human_version, game_version, Some("fabric"), None)
}

pub(crate) fn release_wire_for_game_with_loader(
	signer: &SigningKey,
	project_id: &str,
	nonce: u8,
	human_version: &str,
	game_version: &str,
	loader: Option<&str>,
	loader_version: Option<&str>,
) -> (Vec<u8>, [u8; 32]) {
	release_wire_for_game_id(
		signer,
		project_id,
		nonce,
		human_version,
		game_version,
		"minecraft",
		loader,
		loader_version,
	)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn release_wire_for_game_id(
	signer: &SigningKey,
	project_id: &str,
	nonce: u8,
	human_version: &str,
	game_version: &str,
	game_id: &str,
	loader: Option<&str>,
	loader_version: Option<&str>,
) -> (Vec<u8>, [u8; 32]) {
	let release = ReleasePayload {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: sample_id(game_id),
		release_nonce: vec![nonce; 16],
		human_version: human_version.to_string(),
		channel: "release".to_string(),
		kind: "mod".to_string(),
		declared_time: 1_760_000_000,
		compatibility: vec![Compatibility {
			game_version_predicate: Predicate::new(Scheme::Exact, vec![game_version.to_string()]),
			loader_id: loader.map(sample_id),
			loader_version_predicate: loader_version.map(|version| Predicate::new(Scheme::Exact, vec![version.to_string()])),
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
	let signed = sign_payload(Kind::Release, &release, &[signer]);
	let digest = object_id(Kind::Release, &signed.payload_bytes);
	(signed.wire_bytes(), digest)
}

pub(crate) fn release_wire_with_channel(
	signer: &SigningKey,
	project_id: &str,
	nonce: u8,
	human_version: &str,
	channel: &str,
) -> (Vec<u8>, [u8; 32]) {
	let release = ReleasePayload {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: sample_id("minecraft"),
		release_nonce: vec![nonce; 16],
		human_version: human_version.to_string(),
		channel: channel.to_string(),
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
	let signed = sign_payload(Kind::Release, &release, &[signer]);
	let digest = object_id(Kind::Release, &signed.payload_bytes);
	(signed.wire_bytes(), digest)
}

pub(crate) fn release_wire_with_runtime(
	signer: &SigningKey,
	project_id: &str,
	nonce: u8,
	human_version: &str,
	runtime_predicate: Option<Predicate>,
) -> (Vec<u8>, [u8; 32]) {
	let release = ReleasePayload {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: sample_id("minecraft"),
		release_nonce: vec![nonce; 16],
		human_version: human_version.to_string(),
		channel: "release".to_string(),
		kind: "mod".to_string(),
		declared_time: 1_760_000_000,
		compatibility: vec![Compatibility {
			game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
			loader_id: None,
			loader_version_predicate: None,
			side: Side::Both,
			runtime_predicate,
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
	let signed = sign_payload(Kind::Release, &release, &[signer]);
	let digest = object_id(Kind::Release, &signed.payload_bytes);
	(signed.wire_bytes(), digest)
}

pub(crate) fn release_wire_with_changelog(
	signer: &SigningKey,
	project_id: &str,
	nonce: u8,
	human_version: &str,
	changelog_digest: [u8; 32],
) -> (Vec<u8>, [u8; 32]) {
	let release = ReleasePayload {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: sample_id("minecraft"),
		release_nonce: vec![nonce; 16],
		human_version: human_version.to_string(),
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
		changelog_digest: Some(changelog_digest.to_vec()),
		license_expression: None,
		rights: None,
		sbom_digest: None,
		minimum_verifier_version: 1,
		critical_extensions: Vec::new(),
	};
	let signed = sign_payload(Kind::Release, &release, &[signer]);
	let digest = object_id(Kind::Release, &signed.payload_bytes);
	(signed.wire_bytes(), digest)
}

pub(crate) fn release_wire_no_loader(
	signer: &SigningKey,
	project_id: &str,
	nonce: u8,
	human_version: &str,
) -> (Vec<u8>, [u8; 32]) {
	release_wire_for_game_with_loader(signer, project_id, nonce, human_version, "1.20.1", None, None)
}

pub(crate) fn feed_wire(
	signer: &SigningKey,
	project_id: &str,
	sequence: u64,
	previous: Option<[u8; 32]>,
	object_digest: [u8; 32],
) -> Vec<u8> {
	feed_wire_kind(signer, project_id, sequence, previous, object_digest, "release-published")
}

pub(crate) fn feed_wire_kind(
	signer: &SigningKey,
	project_id: &str,
	sequence: u64,
	previous: Option<[u8; 32]>,
	object_digest: [u8; 32],
	kind: &str,
) -> Vec<u8> {
	let entry = FeedEntry {
		protocol: 1,
		project_id: project_id.to_string(),
		sequence,
		previous: previous.map(|digest| digest.to_vec()),
		kind: kind.to_string(),
		object_digest: object_digest.to_vec(),
		declared_at: 1_760_000_000 + sequence as i64,
	};
	sign_payload(Kind::FeedEntry, &entry, &[signer]).wire_bytes()
}
