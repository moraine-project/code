use moraine_codec::{Value, encode};
use moraine_crypto::{ObjectKind, SigningKey, object_id_string};
use moraine_model::artifact::Artifact;
use moraine_model::canonical::Canonical;
use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
use moraine_model::delegation::{Delegation, KeyDelegation, OwnerRef, OwnershipTransfer};
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::profile::ProfileRevision;
use moraine_model::release::ReleasePayload;
use moraine_model::signed::{SignatureEnvelope, SignedObject, sign_payload};
use moraine_model::version::VersionCatalog;
use sha2::{Digest, Sha256};

use crate::vector::{EnvelopeJson, PredicateCase, SignatureJson, TrustJson, Vector, VectorFile};

pub(crate) const DECLARED_AT: i64 = 1_760_000_000;

pub(crate) fn signer(byte: u8) -> SigningKey {
	SigningKey::from_seed(&[byte; 32])
}

pub(crate) fn public_hex(key: &SigningKey) -> String {
	hex::encode(key.verifying_key().to_bytes())
}

pub(crate) fn sample_id(label: &str) -> String {
	format!("gd:sha256:{}", hex::encode(Sha256::digest(label.as_bytes())))
}

pub(crate) fn envelope_json(envelope: &SignatureEnvelope) -> EnvelopeJson {
	EnvelopeJson {
		alg: envelope.alg,
		signatures: envelope
			.signatures
			.iter()
			.map(|signature| SignatureJson {
				alg: signature.alg,
				key_id: hex::encode(signature.key_id.digest()),
				sig: hex::encode(&signature.sig),
			})
			.collect(),
		key_ids: envelope.key_ids.iter().map(|key_id| hex::encode(key_id.digest())).collect(),
	}
}

pub(crate) fn tampered_envelope(envelope: &SignatureEnvelope) -> EnvelopeJson {
	let mut json = envelope_json(envelope);
	if let Some(signature) = json.signatures.first_mut() {
		let mut bytes = hex::decode(&signature.sig).expect("signature hex");
		let last = bytes.len() - 1;
		bytes[last] ^= 0x01;
		signature.sig = hex::encode(bytes);
	}
	json
}

pub(crate) fn trust(keys: &[&SigningKey], threshold: usize) -> TrustJson {
	TrustJson {
		roots: keys.iter().map(|key| public_hex(key)).collect(),
		threshold,
		authorized_kinds: Vec::new(),
		profile_roots: Vec::new(),
		delegated_keys: Vec::new(),
	}
}

pub(crate) fn object_vector<T: Canonical>(
	name: &str,
	category: &str,
	kind: ObjectKind,
	signed: &SignedObject<T>,
	verdict: &str,
	reason: Option<&str>,
	trust: Option<TrustJson>,
) -> Vector {
	Vector {
		name: name.to_string(),
		category: category.to_string(),
		kind: kind.as_str().to_string(),
		payload_hex: hex::encode(&signed.payload_bytes),
		envelope: Some(envelope_json(&signed.envelope)),
		expected_id: Some(signed.id(kind)),
		expected_verdict: verdict.to_string(),
		reason_code: reason.map(str::to_string),
		verify_kind: None,
		trust,
		prior_feed_hex: Vec::new(),
		predicate_case: None,
	}
}

pub(crate) fn predicate_vector(
	name: &str,
	catalog: VersionCatalog,
	predicate: Predicate,
	version: &str,
	expected: &str,
) -> Vector {
	Vector {
		name: name.to_string(),
		category: "definitions".to_string(),
		kind: "predicate".to_string(),
		payload_hex: String::new(),
		envelope: None,
		expected_id: None,
		expected_verdict: "accept".to_string(),
		reason_code: None,
		verify_kind: None,
		trust: None,
		prior_feed_hex: Vec::new(),
		predicate_case: Some(PredicateCase {
			ordering: catalog.scheme().as_str().to_string(),
			catalog: catalog.ordered().to_vec(),
			scheme: predicate.scheme.clone(),
			values: predicate.values.clone(),
			version: version.to_string(),
			expected: expected.to_string(),
		}),
	}
}

#[derive(Clone, Copy)]
pub(crate) struct Outcome<'a> {
	pub verdict: &'a str,
	pub reason: Option<&'a str>,
}

pub(crate) fn raw_vector(
	name: &str,
	category: &str,
	kind: ObjectKind,
	value: Value,
	keys: &[&SigningKey],
	outcome: Outcome<'_>,
	trust: Option<TrustJson>,
) -> Vector {
	let payload_bytes = encode(&value).expect("value is encodable");
	let envelope = moraine_model::signed::sign_raw(kind, &payload_bytes, keys);
	Vector {
		name: name.to_string(),
		category: category.to_string(),
		kind: kind.as_str().to_string(),
		payload_hex: hex::encode(&payload_bytes),
		envelope: Some(envelope_json(&envelope)),
		expected_id: Some(object_id_string(kind, &payload_bytes)),
		expected_verdict: outcome.verdict.to_string(),
		reason_code: outcome.reason.map(str::to_string),
		verify_kind: None,
		trust,
		prior_feed_hex: Vec::new(),
		predicate_case: None,
	}
}

pub(crate) fn canonical_vector(name: &str, bytes: &[u8], verdict: &str, reason: Option<&str>) -> Vector {
	Vector {
		name: name.to_string(),
		category: "canonical".to_string(),
		kind: "canonical".to_string(),
		payload_hex: hex::encode(bytes),
		envelope: None,
		expected_id: None,
		expected_verdict: verdict.to_string(),
		reason_code: reason.map(str::to_string),
		verify_kind: None,
		trust: None,
		prior_feed_hex: Vec::new(),
		predicate_case: None,
	}
}

fn build_genesis(kind: GenesisKind, roots: &[&SigningKey], threshold: u32, kinds: &[&str], nonce: u8) -> Genesis {
	Genesis {
		protocol: 1,
		kind,
		nonce: vec![nonce; 16],
		roots: roots
			.iter()
			.map(|key| RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).expect("valid root key"))
			.collect(),
		threshold,
		authorized_kinds: kinds.iter().map(|kind| kind.to_string()).collect(),
		home_hint: None,
		contacts: None,
		created_at: DECLARED_AT,
	}
}

fn build_compatibility() -> Compatibility {
	Compatibility {
		game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
		loader_id: Some(sample_id("fabric")),
		loader_version_predicate: None,
		side: Side::Both,
		runtime_predicate: None,
		os_predicate: None,
		arch_predicate: None,
	}
}

fn build_artifact(digest_byte: u8, primary: bool) -> Artifact {
	Artifact {
		digest: vec![digest_byte; 32],
		size: 4096,
		media_type: "application/java-archive".to_string(),
		filename: format!("example-{digest_byte}.jar"),
		is_primary: primary,
		os_predicate: None,
		arch_predicate: None,
	}
}

fn build_release(project_id: &str, artifacts: Vec<Artifact>, critical: Vec<String>) -> ReleasePayload {
	ReleasePayload {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: sample_id("minecraft"),
		release_nonce: vec![0x42; 16],
		human_version: "1.4.0".to_string(),
		channel: "release".to_string(),
		kind: "mod".to_string(),
		declared_time: DECLARED_AT,
		compatibility: vec![build_compatibility()],
		artifacts,
		dependencies: Vec::new(),
		source_reference: None,
		changelog_digest: None,
		license_expression: Some("MIT".to_string()),
		rights: None,
		sbom_digest: None,
		minimum_verifier_version: 1,
		critical_extensions: critical,
	}
}

fn build_profile(project_id: &str) -> ProfileRevision {
	ProfileRevision {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: sample_id("minecraft"),
		revision_nonce: vec![0x24; 16],
		display_name: "Example Mod".to_string(),
		summary: "A worked example".to_string(),
		description: "Longer description".to_string(),
		icon: None,
		gallery: Vec::new(),
		links: Vec::new(),
		communities: Vec::new(),
		categories: vec!["utility".to_string()],
		tags: vec!["client".to_string()],
		rights: None,
		declared_time: DECLARED_AT,
	}
}

fn build_key_delegation(project_id: &str, delegate: &SigningKey, allowed_kinds: &[&str]) -> Delegation {
	Delegation::Key(KeyDelegation {
		protocol: 1,
		project_id: project_id.to_string(),
		delegate_key: RootKey::from_public_key(delegate.verifying_key().to_bytes().to_vec()).expect("delegate"),
		allowed_kinds: allowed_kinds.iter().map(|kind| kind.to_string()).collect(),
		channels: None,
		max_version_scope: None,
		valid_from_seq: None,
		expires_at: None,
		issued_at: DECLARED_AT,
		previous_delegation_digest: None,
	})
}

fn build_transfer(project_id: &str) -> Delegation {
	Delegation::OwnershipTransfer(OwnershipTransfer {
		protocol: 1,
		project_id: project_id.to_string(),
		from_owner: OwnerRef {
			kind: "user".to_string(),
			id: "user-a".to_string(),
		},
		to_owner: OwnerRef {
			kind: "org".to_string(),
			id: "org-b".to_string(),
		},
		issued_at: DECLARED_AT,
		previous_delegation_digest: None,
	})
}

fn build_feed(project_id: &str, sequence: u64, previous: Option<Vec<u8>>) -> FeedEntry {
	FeedEntry {
		protocol: 1,
		project_id: project_id.to_string(),
		sequence,
		previous,
		kind: "release-published".to_string(),
		object_digest: vec![0xCD; 32],
		declared_at: DECLARED_AT,
	}
}

pub fn generate() -> VectorFile {
	let k1 = signer(1);
	let k2 = signer(2);
	let project_id = sample_id("example-project");

	let mut vectors = Vec::new();

	let good = encode(&Value::map([(Value::text("a"), Value::int(1))])).expect("encodable");
	vectors.push(canonical_vector("canonical-minimal-map", &good, "accept", None));
	vectors.push(canonical_vector(
		"canonical-duplicate-key",
		&[0xa2, 0x61, 0x61, 0x01, 0x61, 0x61, 0x02],
		"reject",
		Some("duplicate-key"),
	));
	vectors.push(canonical_vector(
		"canonical-non-minimal-integer",
		&[0x18, 0x17],
		"reject",
		Some("non-canonical-encoding"),
	));
	vectors.push(canonical_vector(
		"canonical-unsorted-keys",
		&[0xa2, 0x61, 0x62, 0x01, 0x61, 0x61, 0x02],
		"reject",
		Some("non-canonical-encoding"),
	));
	vectors.push(canonical_vector(
		"canonical-float",
		&[0xf9, 0x00, 0x00],
		"reject",
		Some("invalid-encoding"),
	));
	vectors.push(canonical_vector(
		"canonical-normalization-collision",
		&[0xa2, 0x62, 0xc3, 0xa9, 0x01, 0x63, 0x65, 0xcc, 0x81, 0x02],
		"reject",
		Some("non-canonical-encoding"),
	));

	let genesis = build_genesis(GenesisKind::Project, &[&k1], 1, &["delegation", "release", "profile"], 0x11);
	let signed = sign_payload(ObjectKind::Genesis, &genesis, &[&k1]);
	vectors.push(object_vector(
		"genesis-project-root-signed",
		"genesis",
		ObjectKind::Genesis,
		&signed,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let game = build_genesis(GenesisKind::Game, &[&k1], 1, &["delegation", "game-def"], 0x12);
	let signed_game = sign_payload(ObjectKind::Genesis, &game, &[&k1]);
	vectors.push(object_vector(
		"genesis-game-kind-requirement",
		"genesis",
		ObjectKind::Genesis,
		&signed_game,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let multi = build_genesis(
		GenesisKind::Project,
		&[&k1, &k2],
		2,
		&["delegation", "release", "profile"],
		0x13,
	);
	let signed_multi = sign_payload(ObjectKind::Genesis, &multi, &[&k1]);
	vectors.push(object_vector(
		"genesis-threshold-not-met",
		"genesis",
		ObjectKind::Genesis,
		&signed_multi,
		"reject",
		Some("threshold-not-met"),
		Some(trust(&[&k1, &k2], 2)),
	));

	let missing_profile = build_genesis(GenesisKind::Project, &[&k1], 1, &["delegation", "release"], 0x14);
	let signed_missing = sign_payload(ObjectKind::Genesis, &missing_profile, &[&k1]);
	vectors.push(object_vector(
		"genesis-missing-required-kind",
		"genesis",
		ObjectKind::Genesis,
		&signed_missing,
		"reject",
		Some("genesis-kind-requirement"),
		Some(trust(&[&k1], 1)),
	));

	let mut bad_env = object_vector(
		"genesis-bad-signature",
		"genesis",
		ObjectKind::Genesis,
		&signed,
		"reject",
		Some("bad-signature"),
		Some(trust(&[&k1], 1)),
	);
	bad_env.envelope = Some(tampered_envelope(&signed.envelope));
	vectors.push(bad_env);

	let mut wrong_id = object_vector(
		"genesis-object-id-mismatch",
		"genesis",
		ObjectKind::Genesis,
		&signed,
		"reject",
		Some("object-id-mismatch"),
		Some(trust(&[&k1], 1)),
	);
	wrong_id.expected_id = Some(sample_id("not-the-project"));
	vectors.push(wrong_id);

	let delegation = build_key_delegation(&project_id, &k2, &["release"]);
	let signed_delegation = sign_payload(ObjectKind::Delegation, &delegation, &[&k1]);
	vectors.push(object_vector(
		"delegation-key-release-scope",
		"delegation",
		ObjectKind::Delegation,
		&signed_delegation,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let unauthorized = build_key_delegation(&project_id, &k2, &["advisory"]);
	let signed_unauthorized = sign_payload(ObjectKind::Delegation, &unauthorized, &[&k1]);
	let mut restricted = trust(&[&k1], 1);
	restricted.authorized_kinds = vec!["delegation".to_string(), "release".to_string(), "profile".to_string()];
	vectors.push(object_vector(
		"delegation-kind-outside-genesis",
		"delegation",
		ObjectKind::Delegation,
		&signed_unauthorized,
		"reject",
		Some("unauthorized-kind"),
		Some(restricted),
	));

	let transfer = build_transfer(&project_id);
	let signed_transfer_two = sign_payload(ObjectKind::Delegation, &transfer, &[&k1, &k2]);
	vectors.push(object_vector(
		"transfer-two-signatures",
		"delegation",
		ObjectKind::Delegation,
		&signed_transfer_two,
		"accept",
		None,
		Some(trust(&[&k1, &k2], 1)),
	));

	let signed_transfer_one = sign_payload(ObjectKind::Delegation, &transfer, &[&k1]);
	vectors.push(object_vector(
		"transfer-one-signature",
		"delegation",
		ObjectKind::Delegation,
		&signed_transfer_one,
		"reject",
		Some("transfer-needs-two-signatures"),
		Some(trust(&[&k1], 1)),
	));

	let release = build_release(&project_id, vec![build_artifact(0x01, true)], Vec::new());
	let signed_release = sign_payload(ObjectKind::Release, &release, &[&k1]);
	vectors.push(object_vector(
		"release-valid-primary",
		"release",
		ObjectKind::Release,
		&signed_release,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let ambiguous = build_release(
		&project_id,
		vec![build_artifact(0x01, true), build_artifact(0x02, true)],
		Vec::new(),
	);
	let signed_ambiguous = sign_payload(ObjectKind::Release, &ambiguous, &[&k1]);
	vectors.push(object_vector(
		"release-ambiguous-primary",
		"release",
		ObjectKind::Release,
		&signed_ambiguous,
		"reject",
		Some("primary-artifact-ambiguous"),
		Some(trust(&[&k1], 1)),
	));

	let critical = build_release(
		&project_id,
		vec![build_artifact(0x01, true)],
		vec!["moraine.future".to_string()],
	);
	let signed_critical = sign_payload(ObjectKind::Release, &critical, &[&k1]);
	vectors.push(object_vector(
		"release-unknown-critical-extension",
		"release",
		ObjectKind::Release,
		&signed_critical,
		"reject",
		Some("unknown-critical-extension"),
		Some(trust(&[&k1], 1)),
	));

	let mut bad_release = object_vector(
		"release-bad-signature",
		"release",
		ObjectKind::Release,
		&signed_release,
		"reject",
		Some("bad-signature"),
		Some(trust(&[&k1], 1)),
	);
	bad_release.envelope = Some(tampered_envelope(&signed_release.envelope));
	vectors.push(bad_release);

	let profile = build_profile(&project_id);
	let signed_profile = sign_payload(ObjectKind::Profile, &profile, &[&k1]);
	let mut profile_trust = trust(&[&k1], 1);
	profile_trust.profile_roots = vec![public_hex(&k1)];
	vectors.push(object_vector(
		"profile-root-signed",
		"profile",
		ObjectKind::Profile,
		&signed_profile,
		"accept",
		None,
		Some(profile_trust.clone()),
	));

	let signed_profile_delegated = sign_payload(ObjectKind::Profile, &profile, &[&k2]);
	let mut delegated_trust = trust(&[&k1], 1);
	delegated_trust.profile_roots = vec![public_hex(&k1)];
	delegated_trust.delegated_keys = vec![public_hex(&k2)];
	vectors.push(object_vector(
		"profile-signed-by-non-profile-key",
		"profile",
		ObjectKind::Profile,
		&signed_profile_delegated,
		"reject",
		Some("profile-authority"),
		Some(delegated_trust),
	));

	let mut cross = object_vector(
		"cross-object-substitution",
		"cross-object",
		ObjectKind::Delegation,
		&signed_delegation,
		"reject",
		Some("bad-signature"),
		Some(trust(&[&k1], 1)),
	);
	cross.verify_kind = Some("release".to_string());
	vectors.push(cross);

	let feed1 = build_feed(&project_id, 1, None);
	let signed_feed1 = sign_payload(ObjectKind::FeedEntry, &feed1, &[&k1]);
	vectors.push(object_vector(
		"feed-first-entry",
		"feed",
		ObjectKind::FeedEntry,
		&signed_feed1,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let feed_bad_first = build_feed(&project_id, 1, Some(vec![0x00; 32]));
	let signed_bad_first = sign_payload(ObjectKind::FeedEntry, &feed_bad_first, &[&k1]);
	vectors.push(object_vector(
		"feed-first-entry-with-previous",
		"feed",
		ObjectKind::FeedEntry,
		&signed_bad_first,
		"reject",
		Some("previous-mismatch"),
		Some(trust(&[&k1], 1)),
	));

	let feed2 = build_feed(&project_id, 2, Some(feed1.id_bytes().to_vec()));
	let signed_feed2 = sign_payload(ObjectKind::FeedEntry, &feed2, &[&k1]);
	let mut feed2_vector = object_vector(
		"feed-second-entry-linked",
		"feed",
		ObjectKind::FeedEntry,
		&signed_feed2,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	);
	feed2_vector.prior_feed_hex = vec![hex::encode(signed_feed1.payload_bytes.clone())];
	vectors.push(feed2_vector);

	let feed_wrong_previous = build_feed(&project_id, 2, Some(vec![0xEE; 32]));
	let signed_wrong_previous = sign_payload(ObjectKind::FeedEntry, &feed_wrong_previous, &[&k1]);
	let mut wrong_previous_vector = object_vector(
		"feed-wrong-previous",
		"feed",
		ObjectKind::FeedEntry,
		&signed_wrong_previous,
		"reject",
		Some("previous-mismatch"),
		Some(trust(&[&k1], 1)),
	);
	wrong_previous_vector.prior_feed_hex = vec![hex::encode(signed_feed1.payload_bytes.clone())];
	vectors.push(wrong_previous_vector);

	let feed_gap = build_feed(&project_id, 5, Some(feed1.id_bytes().to_vec()));
	let signed_gap = sign_payload(ObjectKind::FeedEntry, &feed_gap, &[&k1]);
	let mut gap_vector = object_vector(
		"feed-sequence-gap",
		"feed",
		ObjectKind::FeedEntry,
		&signed_gap,
		"reject",
		Some("sequence-gap"),
		Some(trust(&[&k1], 1)),
	);
	gap_vector.prior_feed_hex = vec![hex::encode(signed_feed1.payload_bytes.clone())];
	vectors.push(gap_vector);

	vectors.extend(crate::definitions::vectors());
	vectors.extend(crate::records::vectors());

	VectorFile {
		protocol: 1,
		description:
			"Moraine protocol test vectors: canonical encoding, genesis, delegation, release, profile, cross-object, and feed"
				.to_string(),
		vectors,
	}
}
