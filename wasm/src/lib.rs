use moraine_crypto::{ObjectKind, SigningKey};
use moraine_model::Canonical;
use moraine_model::artifact::Artifact;
use moraine_model::changelog::{Changelog, ChangelogSection, LocaleSection};
use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
use moraine_model::delegation::{Delegation, OwnerRef, OwnershipTransfer};
use moraine_model::dependency::TargetKind;
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::modpack::{ModpackEntry, ModpackManifest, ModpackOverride};
use moraine_model::profile::ProfileRevision;
use moraine_model::release::{ReleasePayload, Withdrawal};
use moraine_model::signed::{ObjectPayload, SignedObject, sign_payload};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

struct Signed {
	wire: Vec<u8>,
	id: String,
}

impl Signed {
	fn json(self) -> String {
		serde_json::json!({ "wire": hex::encode(self.wire), "id": self.id }).to_string()
	}
}

fn seed(hex_seed: &str) -> Result<zeroize::Zeroizing<[u8; 32]>, String> {
	let bytes = zeroize::Zeroizing::new(hex::decode(hex_seed.trim()).map_err(|_| "the key is not hex".to_string())?);
	if bytes.len() != 32 {
		return Err("the key must be 32 bytes".to_string());
	}
	let mut seed = zeroize::Zeroizing::new([0u8; 32]);
	seed.copy_from_slice(&bytes);
	Ok(seed)
}

fn bytes(value: &str, what: &str) -> Result<Vec<u8>, String> {
	let trimmed = value.trim();
	if trimmed.len() > 128 {
		return Err(format!("{what} is too long"));
	}
	hex::decode(trimmed).map_err(|_| format!("{what} is not hex"))
}

fn digest(value: &str, what: &str) -> Result<Vec<u8>, String> {
	let trimmed = value.trim();
	let hex = trimmed
		.strip_prefix("gd:sha256:")
		.or_else(|| trimmed.strip_prefix("sha256:"))
		.unwrap_or(trimmed);
	bytes(hex, what)
}

fn finish<T: Canonical + Clone + ObjectPayload>(kind: ObjectKind, payload: &T, seed: &[u8; 32]) -> Result<Signed, String> {
	let key = SigningKey::from_seed(seed);
	let signed = sign_payload(kind, payload, &[&key]).map_err(|error| format!("payload cannot be signed: {error}"))?;
	Ok(Signed {
		id: signed.id(kind),
		wire: signed.wire_bytes(),
	})
}

fn side(value: &str) -> Result<Side, String> {
	match value.trim().to_ascii_lowercase().as_str() {
		"client" => Ok(Side::Client),
		"server" => Ok(Side::Server),
		"both" | "" => Ok(Side::Both),
		other => Err(format!("unknown side `{other}`")),
	}
}

fn markdown_sections(body: &str) -> Vec<ChangelogSection> {
	let mut sections: Vec<ChangelogSection> = Vec::new();
	for line in body.lines() {
		if let Some(heading) = line.trim().strip_prefix('#') {
			let heading = heading.trim_start_matches('#').trim();
			if !heading.is_empty() {
				sections.push(ChangelogSection {
					heading: heading.to_string(),
					body: String::new(),
					severity: None,
				});
				continue;
			}
		}
		match sections.last_mut() {
			Some(section) => {
				if !section.body.is_empty() {
					section.body.push('\n');
				}
				section.body.push_str(line);
			}
			None => sections.push(ChangelogSection {
				heading: "Notes".to_string(),
				body: line.to_string(),
				severity: None,
			}),
		}
	}
	for section in &mut sections {
		section.body = section.body.trim().to_string();
	}
	if sections.is_empty() {
		sections.push(ChangelogSection {
			heading: "Notes".to_string(),
			body: String::new(),
			severity: None,
		});
	}
	sections
}

#[derive(Deserialize)]
struct GenesisInput {
	nonce: String,
	authorized_kinds: Vec<String>,
	#[serde(default)]
	additional_root_seeds: Vec<String>,
	#[serde(default = "one")]
	threshold: u32,
	#[serde(default)]
	home_hint: Option<String>,
	created_at: i64,
}

fn one() -> u32 {
	1
}

fn build_genesis(input: &GenesisInput, signing_seed: &[u8; 32]) -> Result<Signed, String> {
	let key = SigningKey::from_seed(signing_seed);
	let mut roots =
		vec![RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).map_err(|error| error.to_string())?];
	for additional_seed in &input.additional_root_seeds {
		let additional_seed = seed(additional_seed)?;
		let additional_key = SigningKey::from_seed(&additional_seed);
		roots.push(
			RootKey::from_public_key(additional_key.verifying_key().to_bytes().to_vec())
				.map_err(|error| error.to_string())?,
		);
	}
	let genesis = Genesis {
		protocol: 1,
		kind: GenesisKind::Project,
		nonce: bytes(&input.nonce, "the nonce")?,
		roots,
		threshold: input.threshold.max(1),
		authorized_kinds: input.authorized_kinds.clone(),
		home_hint: input.home_hint.clone(),
		contacts: None,
		created_at: input.created_at,
	};
	finish(ObjectKind::Genesis, &genesis, signing_seed)
}

#[derive(Deserialize)]
struct ReleaseInput {
	project_id: String,
	game_id: String,
	nonce: String,
	human_version: String,
	#[serde(default = "release_channel")]
	channel: String,
	#[serde(default = "mod_kind")]
	kind: String,
	declared_time: i64,
	#[serde(default)]
	game_versions: Vec<String>,
	#[serde(default)]
	loader_id: Option<String>,
	#[serde(default = "both")]
	side: String,
	digest: String,
	size: u64,
	#[serde(default = "java_archive")]
	media_type: String,
	filename: String,
	#[serde(default)]
	changelog_digest: Option<String>,
	#[serde(default)]
	license_expression: Option<String>,
}

fn release_channel() -> String {
	"release".to_string()
}

fn mod_kind() -> String {
	"mod".to_string()
}

fn both() -> String {
	"both".to_string()
}

fn java_archive() -> String {
	"application/java-archive".to_string()
}

fn build_release(input: &ReleaseInput, seed: &[u8; 32]) -> Result<Signed, String> {
	if input.game_versions.is_empty() {
		return Err("at least one game version is required".to_string());
	}
	let release = ReleasePayload {
		protocol: 1,
		project_id: input.project_id.clone(),
		game_id: input.game_id.clone(),
		release_nonce: bytes(&input.nonce, "the nonce")?,
		human_version: input.human_version.clone(),
		channel: input.channel.clone(),
		kind: input.kind.clone(),
		declared_time: input.declared_time,
		compatibility: vec![Compatibility {
			game_version_predicate: Predicate::new(Scheme::Exact, input.game_versions.clone()),
			loader_id: input.loader_id.clone(),
			loader_version_predicate: None,
			side: side(&input.side)?,
			runtime_predicate: None,
			os_predicate: None,
			arch_predicate: None,
		}],
		artifacts: vec![Artifact {
			digest: digest(&input.digest, "the artifact digest")?,
			size: input.size,
			media_type: input.media_type.clone(),
			filename: input.filename.clone(),
			is_primary: true,
			os_predicate: None,
			arch_predicate: None,
		}],
		dependencies: Vec::new(),
		source_reference: None,
		changelog_digest: input
			.changelog_digest
			.as_deref()
			.map(|value| digest(value, "the changelog digest"))
			.transpose()?,
		license_expression: input.license_expression.clone(),
		rights: None,
		sbom_digest: None,
		minimum_verifier_version: 1,
		critical_extensions: Vec::new(),
	};
	finish(ObjectKind::Release, &release, seed)
}

#[derive(Deserialize)]
struct ProfileInput {
	project_id: String,
	game_id: String,
	nonce: String,
	display_name: String,
	#[serde(default)]
	summary: String,
	#[serde(default)]
	description: String,
	declared_time: i64,
	#[serde(default)]
	categories: Vec<String>,
	#[serde(default)]
	tags: Vec<String>,
}

fn build_profile(input: &ProfileInput, seed: &[u8; 32]) -> Result<Signed, String> {
	let profile = ProfileRevision {
		protocol: 1,
		project_id: input.project_id.clone(),
		game_id: input.game_id.clone(),
		revision_nonce: bytes(&input.nonce, "the nonce")?,
		display_name: input.display_name.clone(),
		summary: input.summary.clone(),
		description: input.description.clone(),
		icon: None,
		gallery: Vec::new(),
		links: Vec::new(),
		communities: Vec::new(),
		categories: input.categories.clone(),
		tags: input.tags.clone(),
		rights: None,
		declared_time: input.declared_time,
	};
	finish(ObjectKind::Profile, &profile, seed)
}

#[derive(Deserialize)]
struct ChangelogInput {
	project_id: String,
	#[serde(default)]
	release_id: Option<String>,
	#[serde(default = "english")]
	locale: String,
	body: String,
	declared_time: i64,
}

fn english() -> String {
	"en".to_string()
}

fn build_changelog(input: &ChangelogInput, seed: &[u8; 32]) -> Result<Signed, String> {
	let changelog = Changelog {
		protocol: 1,
		project_id: input.project_id.clone(),
		release_id: input.release_id.clone(),
		locale_sections: vec![LocaleSection {
			locale: input.locale.clone(),
			sections: markdown_sections(&input.body),
		}],
		declared_time: input.declared_time,
	};
	finish(ObjectKind::Changelog, &changelog, seed)
}

#[derive(Deserialize)]
struct ModpackEntryInput {
	ordinal: u32,
	target_kind: String,
	target_id: String,
	release_id: String,
	digest: String,
	#[serde(default = "both")]
	applies_to: String,
}

#[derive(Deserialize)]
struct ModpackOverrideInput {
	digest: String,
	target_path: String,
	#[serde(default = "both")]
	applies_to: String,
}

#[derive(Deserialize)]
struct ModpackInput {
	protocol: u32,
	project_id: String,
	game_id: String,
	#[serde(default)]
	loader_id: Option<String>,
	entries: Vec<ModpackEntryInput>,
	#[serde(default)]
	overrides: Vec<ModpackOverrideInput>,
	#[serde(default)]
	server_manifest_digest: Option<String>,
	declared_time: i64,
}

fn target_kind(value: &str) -> Result<TargetKind, String> {
	TargetKind::parse(value).ok_or_else(|| format!("unknown modpack target kind `{value}`"))
}

fn build_modpack(input: &ModpackInput, seed: &[u8; 32]) -> Result<Signed, String> {
	let manifest = ModpackManifest {
		protocol: input.protocol,
		project_id: input.project_id.clone(),
		game_id: input.game_id.clone(),
		loader_id: input.loader_id.clone(),
		entries: input
			.entries
			.iter()
			.map(|entry| {
				Ok(ModpackEntry {
					ordinal: entry.ordinal,
					target_kind: target_kind(&entry.target_kind)?,
					target_id: entry.target_id.clone(),
					release_id: entry.release_id.clone(),
					digest: digest(&entry.digest, "the modpack entry digest")?,
					applies_to: side(&entry.applies_to)?,
				})
			})
			.collect::<Result<Vec<_>, String>>()?,
		overrides: input
			.overrides
			.iter()
			.map(|override_file| {
				Ok(ModpackOverride {
					digest: digest(&override_file.digest, "the override digest")?,
					target_path: override_file.target_path.clone(),
					applies_to: side(&override_file.applies_to)?,
				})
			})
			.collect::<Result<Vec<_>, String>>()?,
		server_manifest_digest: input
			.server_manifest_digest
			.as_deref()
			.map(|value| digest(value, "the server manifest digest"))
			.transpose()?,
		declared_time: input.declared_time,
	};
	manifest.validate().map_err(|error| error.to_string())?;
	finish(ObjectKind::Modpack, &manifest, seed)
}

#[derive(Deserialize)]
struct WithdrawalInput {
	project_id: String,
	release_id: String,
	reason: String,
	#[serde(default)]
	note: Option<String>,
	declared_time: i64,
}

fn build_withdrawal(input: &WithdrawalInput, seed: &[u8; 32]) -> Result<Signed, String> {
	let withdrawal = Withdrawal {
		protocol: 1,
		project_id: input.project_id.clone(),
		release_id: input.release_id.clone(),
		reason: input.reason.clone(),
		note: input.note.clone(),
		declared_time: input.declared_time,
	};
	withdrawal.validate().map_err(|error| error.to_string())?;
	finish(ObjectKind::Release, &withdrawal, seed)
}

#[derive(Deserialize)]
struct TransferInput {
	project_id: String,
	from_kind: String,
	from_id: String,
	to_kind: String,
	to_id: String,
	issued_at: i64,
}

fn owner(kind: &str, id: &str, key_id: moraine_crypto::KeyId) -> Result<OwnerRef, String> {
	if !matches!(kind, "user" | "org") || id.trim().is_empty() {
		return Err("owner kind must be user or org and owner id is required".to_string());
	}
	Ok(OwnerRef {
		kind: kind.to_string(),
		id: id.to_string(),
		key_id,
	})
}

fn build_transfer(input: &TransferInput, old_seed: &[u8; 32], new_seed: &[u8; 32]) -> Result<Signed, String> {
	let transfer = OwnershipTransfer {
		protocol: 1,
		project_id: input.project_id.clone(),
		from_owner: owner(&input.from_kind, &input.from_id, SigningKey::from_seed(old_seed).key_id())?,
		to_owner: owner(&input.to_kind, &input.to_id, SigningKey::from_seed(new_seed).key_id())?,
		issued_at: input.issued_at,
		previous_delegation_digest: None,
	};
	transfer.validate().map_err(|error| error.to_string())?;
	let transfer = Delegation::OwnershipTransfer(transfer);
	let old = SigningKey::from_seed(old_seed);
	let new = SigningKey::from_seed(new_seed);
	let signed = sign_payload(ObjectKind::Delegation, &transfer, &[&old, &new])
		.map_err(|error| format!("transfer cannot be signed: {error}"))?;
	Ok(Signed {
		id: signed.id(ObjectKind::Delegation),
		wire: signed.wire_bytes(),
	})
}

#[derive(Deserialize)]
struct FeedEntryInput {
	project_id: String,
	sequence: u64,
	#[serde(default)]
	previous: Option<String>,
	kind: String,
	object_digest: String,
	declared_at: i64,
}

fn build_feed_entry(input: &FeedEntryInput, seed: &[u8; 32]) -> Result<Signed, String> {
	let entry = FeedEntry {
		protocol: 1,
		project_id: input.project_id.clone(),
		sequence: input.sequence,
		previous: input
			.previous
			.as_deref()
			.map(|value| digest(value, "the previous entry id"))
			.transpose()?,
		kind: input.kind.clone(),
		object_digest: digest(&input.object_digest, "the object id")?,
		declared_at: input.declared_at,
	};
	finish(ObjectKind::FeedEntry, &entry, seed)
}

fn parse<T: for<'a> Deserialize<'a>>(input: &str) -> Result<T, String> {
	serde_json::from_str(input).map_err(|error| format!("invalid input: {error}"))
}

#[wasm_bindgen]
pub fn public_key(seed_hex: &str) -> Result<String, JsError> {
	let seed = seed(seed_hex).map_err(|error| JsError::new(&error))?;
	Ok(hex::encode(SigningKey::from_seed(&seed).verifying_key().to_bytes()))
}

#[wasm_bindgen]
pub fn key_id(seed_hex: &str) -> Result<String, JsError> {
	let seed = seed(seed_hex).map_err(|error| JsError::new(&error))?;
	Ok(SigningKey::from_seed(&seed).key_id().to_string())
}

#[wasm_bindgen]
pub fn genesis_roots(wire: &[u8]) -> Result<String, JsError> {
	let signed = SignedObject::<Genesis>::from_bytes(wire).map_err(|error| JsError::new(&error.to_string()))?;
	let roots = signed
		.payload
		.roots
		.iter()
		.map(|root| hex::encode(&root.public_key))
		.collect::<Vec<_>>();
	Ok(serde_json::to_string(&roots).expect("roots are serializable"))
}

#[wasm_bindgen]
pub fn sign_genesis(seed_hex: &str, input_json: &str) -> Result<String, JsError> {
	let seed = seed(seed_hex).map_err(|error| JsError::new(&error))?;
	let input: GenesisInput = parse(input_json).map_err(|error| JsError::new(&error))?;
	build_genesis(&input, &seed)
		.map(Signed::json)
		.map_err(|error| JsError::new(&error))
}

#[wasm_bindgen]
pub fn sign_release(seed_hex: &str, input_json: &str) -> Result<String, JsError> {
	let seed = seed(seed_hex).map_err(|error| JsError::new(&error))?;
	let input: ReleaseInput = parse(input_json).map_err(|error| JsError::new(&error))?;
	build_release(&input, &seed)
		.map(Signed::json)
		.map_err(|error| JsError::new(&error))
}

#[wasm_bindgen]
pub fn sign_profile(seed_hex: &str, input_json: &str) -> Result<String, JsError> {
	let seed = seed(seed_hex).map_err(|error| JsError::new(&error))?;
	let input: ProfileInput = parse(input_json).map_err(|error| JsError::new(&error))?;
	build_profile(&input, &seed)
		.map(Signed::json)
		.map_err(|error| JsError::new(&error))
}

#[wasm_bindgen]
pub fn sign_changelog(seed_hex: &str, input_json: &str) -> Result<String, JsError> {
	let seed = seed(seed_hex).map_err(|error| JsError::new(&error))?;
	let input: ChangelogInput = parse(input_json).map_err(|error| JsError::new(&error))?;
	build_changelog(&input, &seed)
		.map(Signed::json)
		.map_err(|error| JsError::new(&error))
}

#[wasm_bindgen]
pub fn sign_modpack(seed_hex: &str, input_json: &str) -> Result<String, JsError> {
	let seed = seed(seed_hex).map_err(|error| JsError::new(&error))?;
	let input: ModpackInput = parse(input_json).map_err(|error| JsError::new(&error))?;
	build_modpack(&input, &seed)
		.map(Signed::json)
		.map_err(|error| JsError::new(&error))
}

#[wasm_bindgen]
pub fn sign_withdrawal(seed_hex: &str, input_json: &str) -> Result<String, JsError> {
	let seed = seed(seed_hex).map_err(|error| JsError::new(&error))?;
	let input: WithdrawalInput = parse(input_json).map_err(|error| JsError::new(&error))?;
	build_withdrawal(&input, &seed)
		.map(Signed::json)
		.map_err(|error| JsError::new(&error))
}

#[wasm_bindgen]
pub fn sign_transfer(old_seed_hex: &str, new_seed_hex: &str, input_json: &str) -> Result<String, JsError> {
	let old_seed = seed(old_seed_hex).map_err(|error| JsError::new(&error))?;
	let new_seed = seed(new_seed_hex).map_err(|error| JsError::new(&error))?;
	let input: TransferInput = parse(input_json).map_err(|error| JsError::new(&error))?;
	build_transfer(&input, &old_seed, &new_seed)
		.map(Signed::json)
		.map_err(|error| JsError::new(&error))
}

#[wasm_bindgen]
pub fn sign_feed_entry(seed_hex: &str, input_json: &str) -> Result<String, JsError> {
	let seed = seed(seed_hex).map_err(|error| JsError::new(&error))?;
	let input: FeedEntryInput = parse(input_json).map_err(|error| JsError::new(&error))?;
	build_feed_entry(&input, &seed)
		.map(Signed::json)
		.map_err(|error| JsError::new(&error))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use moraine_model::signed::TrustedKey;

	use super::*;

	const SEED: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
	const PROJECT: &str = "gd:sha256:1111111111111111111111111111111111111111111111111111111111111111";

	fn seed_bytes() -> zeroize::Zeroizing<[u8; 32]> {
		seed(SEED).expect("seed")
	}

	fn trusted() -> Vec<TrustedKey> {
		let public = SigningKey::from_seed(&seed_bytes()).verifying_key().to_bytes();
		vec![TrustedKey::new(&public).expect("trusted key")]
	}

	fn wire(result: String) -> (Vec<u8>, String) {
		let value: serde_json::Value = serde_json::from_str(&result).expect("json");
		(
			hex::decode(value["wire"].as_str().expect("wire")).expect("hex"),
			value["id"].as_str().expect("id").to_string(),
		)
	}

	#[test]
	fn signs_objects_that_verify_and_carry_the_id_they_report() {
		let genesis = build_genesis(
			&GenesisInput {
				nonce: "00112233445566778899aabbccddeeff".to_string(),
				authorized_kinds: vec!["delegation".to_string(), "release".to_string(), "profile".to_string()],
				additional_root_seeds: Vec::new(),
				threshold: 1,
				home_hint: None,
				created_at: 1_760_000_000,
			},
			&seed_bytes(),
		)
		.expect("genesis");
		let decoded = SignedObject::<Genesis>::from_bytes(&genesis.wire).expect("decode");
		assert_eq!(decoded.id(ObjectKind::Genesis), genesis.id);
		decoded
			.verify_threshold(ObjectKind::Genesis, &trusted(), 1)
			.expect("genesis is signed by the seed");

		let release = build_release(
			&ReleaseInput {
				project_id: PROJECT.to_string(),
				game_id: PROJECT.to_string(),
				nonce: "ffeeddccbbaa99887766554433221100".to_string(),
				human_version: "1.2.3".to_string(),
				channel: "release".to_string(),
				kind: "mod".to_string(),
				declared_time: 1_760_000_000,
				game_versions: vec!["1.20.1".to_string()],
				loader_id: None,
				side: "both".to_string(),
				digest: "ab".repeat(32),
				size: 10,
				media_type: "application/java-archive".to_string(),
				filename: "example.jar".to_string(),
				changelog_digest: None,
				license_expression: None,
			},
			&seed_bytes(),
		)
		.expect("release");
		let decoded = SignedObject::<ReleasePayload>::from_bytes(&release.wire).expect("decode");
		assert_eq!(decoded.id(ObjectKind::Release), release.id);
		decoded
			.verify_threshold(ObjectKind::Release, &trusted(), 1)
			.expect("release is signed by the seed");

		let entry = build_feed_entry(
			&FeedEntryInput {
				project_id: PROJECT.to_string(),
				sequence: 1,
				previous: None,
				kind: "release-published".to_string(),
				object_digest: release.id.clone(),
				declared_at: 1_760_000_001,
			},
			&seed_bytes(),
		)
		.expect("entry");
		let decoded = SignedObject::<FeedEntry>::from_bytes(&entry.wire).expect("decode");
		assert_eq!(decoded.id(ObjectKind::FeedEntry), entry.id);
		decoded
			.verify_threshold(ObjectKind::FeedEntry, &trusted(), 1)
			.expect("feed entry is signed by the seed");
	}

	#[test]
	fn the_same_input_signs_to_the_same_bytes() {
		let profile = || ProfileInput {
			project_id: PROJECT.to_string(),
			game_id: PROJECT.to_string(),
			nonce: "00".repeat(16),
			display_name: "Example".to_string(),
			summary: String::new(),
			description: String::new(),
			declared_time: 1_760_000_000,
			categories: vec!["gameplay".to_string()],
			tags: Vec::new(),
		};
		let first = build_profile(&profile(), &seed_bytes()).expect("profile");
		let second = build_profile(&profile(), &seed_bytes()).expect("profile");
		assert_eq!(first.wire, second.wire);
		assert_eq!(first.id, second.id);
	}

	#[test]
	fn the_wasm_export_returns_the_same_wire_as_the_core() {
		let (wire, id) = wire(
			sign_feed_entry(
				SEED,
				&serde_json::json!({
					"project_id": PROJECT,
					"sequence": 1,
					"kind": "release-published",
					"object_digest": PROJECT,
					"declared_at": 1_760_000_001,
				})
				.to_string(),
			)
			.expect("entry"),
		);
		let core = build_feed_entry(
			&FeedEntryInput {
				project_id: PROJECT.to_string(),
				sequence: 1,
				previous: None,
				kind: "release-published".to_string(),
				object_digest: PROJECT.to_string(),
				declared_at: 1_760_000_001,
			},
			&seed_bytes(),
		)
		.expect("entry");
		assert_eq!(wire, core.wire);
		assert_eq!(id, core.id);
	}

	#[test]
	fn lifecycle_records_are_signed_by_the_browser_boundary() {
		let withdrawal = build_withdrawal(
			&WithdrawalInput {
				project_id: PROJECT.to_string(),
				release_id: PROJECT.to_string(),
				reason: "author-preference".to_string(),
				note: None,
				declared_time: 1_760_000_000,
			},
			&seed_bytes(),
		)
		.expect("withdrawal");
		let decoded = SignedObject::<moraine_model::release::Withdrawal>::from_bytes(&withdrawal.wire).expect("decode");
		decoded
			.verify_threshold(ObjectKind::Release, &trusted(), 1)
			.expect("withdrawal signature");

		let mut new_seed = seed_bytes();
		new_seed[0] ^= 1;
		let transfer = build_transfer(
			&TransferInput {
				project_id: PROJECT.to_string(),
				from_kind: "user".to_string(),
				from_id: "old".to_string(),
				to_kind: "org".to_string(),
				to_id: "new".to_string(),
				issued_at: 1_760_000_000,
			},
			&seed_bytes(),
			&new_seed,
		)
		.expect("transfer");
		let decoded = SignedObject::<Delegation>::from_bytes(&transfer.wire).expect("decode");
		let old_public = SigningKey::from_seed(&seed_bytes()).verifying_key().to_bytes();
		let new_public = SigningKey::from_seed(&new_seed).verifying_key().to_bytes();
		let keys = vec![
			TrustedKey::new(&old_public).expect("old key"),
			TrustedKey::new(&new_public).expect("new key"),
		];
		decoded
			.verify_threshold(ObjectKind::Delegation, &keys, 2)
			.expect("two transfer signatures");
	}

	#[test]
	fn modpack_manifest_is_signed_by_the_browser_boundary() {
		let manifest = build_modpack(
			&ModpackInput {
				protocol: 1,
				project_id: PROJECT.to_string(),
				game_id: PROJECT.to_string(),
				loader_id: None,
				entries: vec![ModpackEntryInput {
					ordinal: 0,
					target_kind: "project".to_string(),
					target_id: PROJECT.to_string(),
					release_id: PROJECT.to_string(),
					digest: "ab".repeat(32),
					applies_to: "both".to_string(),
				}],
				overrides: Vec::new(),
				server_manifest_digest: None,
				declared_time: 1_760_000_000,
			},
			&seed_bytes(),
		)
		.expect("modpack");
		let decoded = SignedObject::<ModpackManifest>::from_bytes(&manifest.wire).expect("decode");
		decoded
			.verify_threshold(ObjectKind::Modpack, &trusted(), 1)
			.expect("modpack signature");
	}
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
	use wasm_bindgen_test::wasm_bindgen_test;

	use super::*;

	const SEED: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

	#[wasm_bindgen_test]
	fn signs_a_release_and_reports_its_id() {
		let project = "gd:sha256:1111111111111111111111111111111111111111111111111111111111111111";
		let result = sign_release(
			SEED,
			&serde_json::json!({
				"project_id": project,
				"game_id": project,
				"nonce": "ffeeddccbbaa99887766554433221100",
				"human_version": "1.2.3",
				"declared_time": 1_760_000_000,
				"game_versions": ["1.20.1"],
				"digest": "ab".repeat(32),
				"size": 10,
				"filename": "example.jar",
			})
			.to_string(),
		)
		.expect("release");
		let value: serde_json::Value = serde_json::from_str(&result).expect("json");
		assert!(value["id"].as_str().expect("id").starts_with("gd:sha256:"));
		assert!(!value["wire"].as_str().expect("wire").is_empty());
	}

	#[wasm_bindgen_test]
	fn refuses_a_key_that_is_not_a_seed() {
		assert!(sign_release("not-hex", "{}").is_err());
	}
}
