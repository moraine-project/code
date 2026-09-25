use std::path::Path;

use moraine_crypto::ObjectKind;
use moraine_model::advisory::{Advisory, Affected, Category, Severity, TAXONOMY_VERSION};
use moraine_model::artifact::Artifact;
use moraine_model::changelog::{Changelog, ChangelogSection, LocaleSection};
use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
use moraine_model::delegation::{Delegation, OwnerRef, OwnershipTransfer};
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::profile::ProfileRevision;
use moraine_model::release::{ReleasePayload, Withdrawal};
use moraine_model::signed::{TrustedKey, sign_payload, try_sign_payload};

use crate::definitions::lock::DefinitionLock;
use crate::home::Home;
use crate::{artifacts, keyfile};

pub fn keygen(path: &Path) -> Result<(), String> {
	let key = keyfile::create(path)?;
	println!("key: {}", key.key_id());
	println!("public: {}", hex::encode(key.verifying_key().to_bytes()));
	println!("written to {}", path.display());
	Ok(())
}

pub async fn init(key_path: &Path, home_url: &str, home_hint: Option<String>) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let genesis = Genesis {
		protocol: 1,
		kind: GenesisKind::Project,
		nonce: random_nonce(),
		roots: vec![RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).map_err(|error| error.to_string())?],
		threshold: 1,
		authorized_kinds: vec![
			"delegation".to_string(),
			"release".to_string(),
			"profile".to_string(),
			"changelog".to_string(),
			"modpack".to_string(),
		],
		home_hint: home_hint.or_else(|| Some(home_url.to_string())),
		contacts: None,
		created_at: now(),
	};
	let signed = sign_payload(ObjectKind::Genesis, &genesis, &[&key])
		.map_err(|error| format!("genesis cannot be signed: {error}"))?;
	let home = Home::new(home_url)?;
	let receipt = home.post_wire("/v1/projects", signed.wire_bytes()).await?;
	println!("project_id: {}", receipt["project_id"].as_str().unwrap_or("?"));
	println!("home: {}", home.base());
	Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn check(
	key_path: &Path,
	home_url: &str,
	project_id: &str,
	game_id: &str,
	game_versions: &[String],
	human_version: &str,
	channel: &str,
	file: &Path,
	loader_id: Option<String>,
	changelog: Option<String>,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let release = release_payload(
		project_id,
		game_id,
		game_versions,
		human_version,
		channel,
		file,
		loader_id,
		changelog,
	)?;
	let signed = try_sign_payload(ObjectKind::Release, &release, &[&key])
		.map_err(|error| format!("release cannot be signed: {error}"))?;
	let trusted = TrustedKey::new(&key.verifying_key().to_bytes()).map_err(|error| format!("key is invalid: {error}"))?;
	signed
		.verify_threshold(ObjectKind::Release, std::slice::from_ref(&trusted), 1)
		.map_err(|error| format!("release signature did not verify: {error}"))?;
	let _ = Home::new(home_url)?;
	println!("release: valid");
	println!("project: {project_id}");
	println!("artifact: sha256:{}", hex::encode(&release.artifacts[0].digest));
	println!("nothing was uploaded; run `release` to store a newly signed object");
	Ok(())
}

#[allow(clippy::too_many_arguments)]
fn release_payload(
	project_id: &str,
	game_id: &str,
	game_versions: &[String],
	human_version: &str,
	channel: &str,
	file: &Path,
	loader_id: Option<String>,
	changelog: Option<String>,
) -> Result<ReleasePayload, String> {
	if game_versions.is_empty() {
		return Err("at least one --game-version is required".to_string());
	}
	let artifact_file = artifacts::describe(file)?;
	let changelog_digest = changelog
		.map(|digest| parse_sha256(&digest).map(|bytes| bytes.to_vec()))
		.transpose()?;
	let release = ReleasePayload {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: game_id.to_string(),
		release_nonce: random_nonce(),
		human_version: human_version.to_string(),
		channel: channel.to_string(),
		kind: "mod".to_string(),
		declared_time: now(),
		compatibility: vec![Compatibility {
			game_version_predicate: Predicate::new(Scheme::Exact, game_versions.to_vec()),
			loader_id,
			loader_version_predicate: None,
			side: Side::Both,
			runtime_predicate: None,
			os_predicate: None,
			arch_predicate: None,
		}],
		artifacts: vec![Artifact {
			digest: artifact_file.digest.to_vec(),
			size: artifact_file.size,
			media_type: artifact_file.media_type,
			filename: artifact_file.filename,
			is_primary: true,
			os_predicate: None,
			arch_predicate: None,
		}],
		dependencies: Vec::new(),
		source_reference: None,
		changelog_digest,
		license_expression: None,
		rights: None,
		sbom_digest: None,
		minimum_verifier_version: 1,
		critical_extensions: Vec::new(),
	};
	release
		.validate()
		.map_err(|error| format!("this release would be rejected: {error}"))?;
	Ok(release)
}

#[allow(clippy::too_many_arguments)]
pub async fn release(
	key_path: &Path,
	home_url: &str,
	project_id: &str,
	game_id: &str,
	game_versions: &[String],
	human_version: &str,
	channel: &str,
	file: &Path,
	loader_id: Option<String>,
	changelog: Option<String>,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let release = release_payload(
		project_id,
		game_id,
		game_versions,
		human_version,
		channel,
		file,
		loader_id,
		changelog,
	)?;
	let signed = try_sign_payload(ObjectKind::Release, &release, &[&key])
		.map_err(|error| format!("release cannot be signed: {error}"))?;
	let home = Home::new(home_url)?;
	let receipt = home
		.post_wire(&format!("/v1/projects/{project_id}/objects/release"), signed.wire_bytes())
		.await?;
	println!("release: {}", receipt["id"].as_str().unwrap_or("?"));
	println!("artifact: sha256:{}", hex::encode(&release.artifacts[0].digest));
	Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn profile(
	key_path: &Path,
	home_url: &str,
	project_id: &str,
	game_id: &str,
	name: &str,
	summary: &str,
	description: &str,
	categories: Vec<String>,
	tags: Vec<String>,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let profile = ProfileRevision {
		protocol: 1,
		project_id: project_id.to_string(),
		game_id: game_id.to_string(),
		revision_nonce: random_nonce(),
		display_name: name.to_string(),
		summary: summary.to_string(),
		description: description.to_string(),
		icon: None,
		gallery: Vec::new(),
		links: Vec::new(),
		communities: Vec::new(),
		categories,
		tags,
		rights: None,
		declared_time: now(),
	};
	let signed = sign_payload(ObjectKind::Profile, &profile, &[&key])
		.map_err(|error| format!("profile cannot be signed: {error}"))?;
	let home = Home::new(home_url)?;
	let receipt = home
		.post_wire(&format!("/v1/projects/{project_id}/objects/profile"), signed.wire_bytes())
		.await?;
	println!("profile: {}", receipt["id"].as_str().unwrap_or("?"));
	println!("next: publish or submit it with --object <profile-id> --kind profile-updated");
	Ok(())
}

pub async fn changelog(
	key_path: &Path,
	home_url: &str,
	project_id: &str,
	release_id: Option<String>,
	locale: &str,
	notes: &Path,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let body = std::fs::read_to_string(notes).map_err(|error| format!("{}: {error}", notes.display()))?;
	let changelog = Changelog {
		protocol: 1,
		project_id: project_id.to_string(),
		release_id,
		locale_sections: vec![LocaleSection {
			locale: locale.to_string(),
			sections: markdown_sections(&body),
		}],
		declared_time: now(),
	};
	let signed = sign_payload(ObjectKind::Changelog, &changelog, &[&key])
		.map_err(|error| format!("changelog cannot be signed: {error}"))?;
	let home = Home::new(home_url)?;
	let receipt = home
		.post_wire(&format!("/v1/projects/{project_id}/objects/changelog"), signed.wire_bytes())
		.await?;
	println!("changelog: {}", receipt["id"].as_str().unwrap_or("?"));
	println!("reference it from a release with --changelog <digest>");
	Ok(())
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

pub fn plan(adapter: &str, mods: &[String], overrides: &[String]) -> Result<(), String> {
	let mod_files = mods
		.iter()
		.map(|entry| parse_placement(entry))
		.collect::<Result<Vec<_>, _>>()?;
	let override_files = overrides
		.iter()
		.map(|entry| parse_placement(entry))
		.collect::<Result<Vec<_>, _>>()?;
	let plan = moraine_install::plan(
		adapter,
		&mod_files
			.into_iter()
			.map(|(name, digest)| moraine_install::ModFile { digest, filename: name })
			.collect::<Vec<_>>(),
		&override_files
			.into_iter()
			.map(|(path, digest)| moraine_install::OverrideFile {
				digest,
				target_path: path,
			})
			.collect::<Vec<_>>(),
	)
	.map_err(|error| error.to_string())?;
	println!("adapter: {}", plan.adapter);
	for placement in plan.placements {
		println!(
			"{} <- sha256:{}",
			placement.relative_path.display(),
			hex::encode(placement.digest)
		);
	}
	Ok(())
}

fn parse_placement(entry: &str) -> Result<(String, [u8; 32]), String> {
	let (name, digest) = entry
		.split_once('=')
		.ok_or_else(|| format!("`{entry}` must be name=sha256:<hex>"))?;
	let digest = parse_sha256(digest)?;
	if name.is_empty() {
		return Err(format!("`{entry}` is missing a name"));
	}
	Ok((name.to_string(), digest))
}

pub fn inspect(file: &Path, extractor: Option<&str>) -> Result<(), String> {
	let bytes = std::fs::read(file).map_err(|error| format!("{}: {error}", file.display()))?;
	let metadata = match extractor {
		Some(extractor) => moraine_metadata::extract_with(extractor, &bytes),
		None => moraine_metadata::extract(&bytes),
	}
	.map_err(|error| error.to_string())?;
	println!("loader: {}", metadata.loader.as_deref().unwrap_or("unknown"));
	println!("mod_id: {}", metadata.mod_id.as_deref().unwrap_or("(none)"));
	println!("name: {}", metadata.name.as_deref().unwrap_or("(none)"));
	println!("version: {}", metadata.version.as_deref().unwrap_or("(none)"));
	if let Some(environment) = &metadata.environment {
		println!("environment: {environment}");
	}
	Ok(())
}

pub async fn provider(key_path: &Path, home_url: &str, provider_id: &str, api_key: Option<String>) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let home = Home::new(home_url)?;
	let body = serde_json::json!({ "public_key": hex::encode(key.verifying_key().to_bytes()) }).to_string();
	let receipt = home
		.post_json_with_token(&format!("/v1/providers/{provider_id}/keys"), body, api_key.as_deref())
		.await?;
	println!("provider: {}", receipt["provider_id"].as_str().unwrap_or("?"));
	Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn advisory(
	key_path: &Path,
	home_url: &str,
	provider_id: &str,
	project_id: &str,
	game_id: &str,
	digest: &str,
	severity: &str,
	category: &str,
	block_promotion: bool,
	evidence_ref: Option<String>,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let advisory = Advisory {
		protocol: 1,
		provider_id: provider_id.to_string(),
		project_id: project_id.to_string(),
		game_id: game_id.to_string(),
		affected: Affected {
			digest: Some(parse_sha256(digest)?.to_vec()),
			predicate: None,
		},
		severity: Severity::parse(severity).ok_or_else(|| format!("unknown severity `{severity}`"))?,
		category: Category::parse(category).ok_or_else(|| format!("unknown category `{category}`"))?,
		taxonomy_version: TAXONOMY_VERSION,
		block_promotion,
		evidence_ref,
		published_at: now(),
		expires_at: None,
		retracted_at: None,
	};
	let signed = sign_payload(ObjectKind::Advisory, &advisory, &[&key])
		.map_err(|error| format!("advisory cannot be signed: {error}"))?;
	let home = Home::new(home_url)?;
	let receipt = home.post_wire("/v1/advisories", signed.wire_bytes()).await?;
	println!("advisory: {}", receipt["advisory"].as_str().unwrap_or("?"));
	Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn attestation(
	key_path: &Path,
	home_url: &str,
	signer_id: &str,
	artifact: &str,
	kind: &str,
	media_type: &str,
	subject_kind: &str,
	subject_id: &str,
	body: Option<&Path>,
	body_digest: Option<&str>,
) -> Result<(), String> {
	use moraine_model::attestation::{Attestation, AttestationKind};

	let key = keyfile::load(key_path)?;
	let kind = AttestationKind::parse(kind).ok_or_else(|| format!("unknown attestation kind `{kind}`"))?;
	let body_inline = match body {
		Some(path) => Some(std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?),
		None => None,
	};
	let body_digest = body_digest.map(parse_sha256).transpose()?.map(|bytes| bytes.to_vec());
	let attestation = Attestation {
		protocol: 1,
		artifact_digest: parse_sha256(artifact)?.to_vec(),
		subject_kind: subject_kind.to_string(),
		subject_id: subject_id.to_string(),
		kind,
		media_type: media_type.to_string(),
		body_digest,
		body_inline,
		signer_id: signer_id.to_string(),
		issued_at: now(),
	};
	let signed = sign_payload(ObjectKind::Attestation, &attestation, &[&key])
		.map_err(|error| format!("attestation cannot be signed: {error}"))?;
	let home = Home::new(home_url)?;
	let receipt = home.post_wire("/v1/attestations", signed.wire_bytes()).await?;
	println!("attestation: {}", receipt["attestation"].as_str().unwrap_or("?"));
	Ok(())
}

fn parse_sha256(value: &str) -> Result<[u8; 32], String> {
	let hex = value
		.strip_prefix("sha256:")
		.or_else(|| value.strip_prefix("gd:sha256:"))
		.unwrap_or(value);
	hex::decode(hex)
		.map_err(|_| format!("`{value}` is not a sha256 digest"))?
		.try_into()
		.map_err(|_| format!("`{value}` must be 32 bytes"))
}

pub async fn withdraw(
	key_path: &Path,
	home_url: &str,
	project_id: &str,
	release_id: &str,
	reason: &str,
	note: Option<String>,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let withdrawal = Withdrawal {
		protocol: 1,
		project_id: project_id.to_string(),
		release_id: release_id.to_string(),
		reason: reason.to_string(),
		note,
		declared_time: now(),
	};
	let signed = sign_payload(ObjectKind::Release, &withdrawal, &[&key])
		.map_err(|error| format!("withdrawal cannot be signed: {error}"))?;
	let home = Home::new(home_url)?;
	let receipt = home
		.post_wire(&format!("/v1/projects/{project_id}/objects/release"), signed.wire_bytes())
		.await?;
	println!("withdrawal: {}", receipt["id"].as_str().unwrap_or("?"));
	println!("next: publish or submit it with --object <withdrawal-id> --kind release-withdrawn");
	Ok(())
}

pub async fn transfer(
	old_key_path: &Path,
	new_key_path: &Path,
	home_url: &str,
	project_id: &str,
	from: &str,
	to: &str,
) -> Result<(), String> {
	let old_key = keyfile::load(old_key_path)?;
	let new_key = keyfile::load(new_key_path)?;
	let transfer = Delegation::OwnershipTransfer(OwnershipTransfer {
		protocol: 1,
		project_id: project_id.to_string(),
		from_owner: parse_owner(from, old_key.key_id())?,
		to_owner: parse_owner(to, new_key.key_id())?,
		issued_at: now(),
		previous_delegation_digest: None,
	});
	let signed = sign_payload(ObjectKind::Delegation, &transfer, &[&old_key, &new_key])
		.map_err(|error| format!("transfer cannot be signed: {error}"))?;
	let home = Home::new(home_url)?;
	let receipt = home
		.post_wire(&format!("/v1/projects/{project_id}/transfer"), signed.wire_bytes())
		.await?;
	println!("transfer: {}", receipt["transfer"].as_str().unwrap_or("?"));
	println!("next: publish or submit it with --object <transfer-id> --kind ownership-transferred");
	Ok(())
}

fn parse_owner(value: &str, key_id: moraine_crypto::KeyId) -> Result<OwnerRef, String> {
	let (kind, id) = value
		.split_once(':')
		.ok_or_else(|| format!("`{value}` must be user:<id> or org:<id>"))?;
	if kind != "user" && kind != "org" {
		return Err(format!("`{value}` must start with user: or org:"));
	}
	if id.is_empty() {
		return Err(format!("`{value}` is missing an id"));
	}
	Ok(OwnerRef {
		kind: kind.to_string(),
		id: id.to_string(),
		key_id,
	})
}

pub async fn upload(home_url: &str, file: &Path, api_key: Option<String>) -> Result<(), String> {
	let metadata = std::fs::metadata(file).map_err(|error| format!("{}: {error}", file.display()))?;
	let handle = tokio::fs::File::open(file)
		.await
		.map_err(|error| format!("{}: {error}", file.display()))?;
	let home = Home::new(home_url)?;
	let receipt = home
		.post_blob("/v1/blobs", handle, metadata.len(), api_key.as_deref())
		.await?;
	println!("digest: {}", receipt["digest"].as_str().unwrap_or("?"));
	println!("size: {}", receipt["size"].as_u64().unwrap_or(0));
	Ok(())
}

pub async fn publish(key_path: &Path, home_url: &str, project_id: &str, object_id: &str, kind: &str) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let home = Home::new(home_url)?;
	let capability = home.get_json("/.well-known/mod-registry").await?;
	if capability["publishing"] != "open" {
		return Err("this home does not accept direct feed publication; use `submit --api-key <token>`".to_string());
	}
	for _ in 0..3 {
		let entry = next_feed_entry(&home, project_id, object_id, kind).await?;
		let signed = try_sign_payload(ObjectKind::FeedEntry, &entry, &[&key])
			.map_err(|error| format!("feed entry cannot be signed: {error}"))?;
		let (status, receipt) = home
			.post_wire_status(&format!("/v1/projects/{project_id}/feed"), signed.wire_bytes())
			.await?;
		if status.is_success() {
			println!("seq: {}", receipt["seq"].as_i64().unwrap_or(0));
			println!("entry: {}", receipt["entry"].as_str().unwrap_or("?"));
			return Ok(());
		}
		if status != reqwest::StatusCode::CONFLICT {
			return Err(format!("home returned {status}"));
		}
	}
	Err("the feed head kept moving while publishing; retry later".to_string())
}

pub async fn submit(
	key_path: &Path,
	home_url: &str,
	project_id: &str,
	object_id: &str,
	kind: &str,
	api_key: Option<String>,
) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let home = Home::new(home_url)?;
	let entry = next_feed_entry(&home, project_id, object_id, kind).await?;
	let signed = try_sign_payload(ObjectKind::FeedEntry, &entry, &[&key])
		.map_err(|error| format!("feed entry cannot be signed: {error}"))?;
	let receipt = home
		.post_wire_with_token("/v1/submissions", signed.wire_bytes(), api_key.as_deref())
		.await?;
	println!("submission: {}", receipt["id"].as_str().unwrap_or("?"));
	println!("state: {}", receipt["state"].as_str().unwrap_or("?"));
	if let Some(seq) = receipt["seq"].as_i64() {
		println!("seq: {seq}");
	}
	Ok(())
}

async fn next_feed_entry(home: &Home, project_id: &str, object_id: &str, kind: &str) -> Result<FeedEntry, String> {
	let project = home.get_json(&format!("/v1/projects/{project_id}")).await?;
	let head_seq = project["head_seq"]
		.as_i64()
		.filter(|sequence| *sequence >= 0)
		.ok_or_else(|| "the home returned an invalid feed head".to_string())?;
	let previous = match project["head_entry"].as_str() {
		Some(entry) => Some(parse_object_id(entry)?.to_vec()),
		None if head_seq == 0 => None,
		None => return Err("the home returned a feed head without its entry".to_string()),
	};
	let sequence = u64::try_from(head_seq)
		.ok()
		.and_then(|sequence| sequence.checked_add(1))
		.ok_or_else(|| "the home's feed sequence is exhausted".to_string())?;
	Ok(FeedEntry {
		protocol: 1,
		project_id: project_id.to_string(),
		sequence,
		previous,
		kind: kind.to_string(),
		object_digest: parse_object_id(object_id)?.to_vec(),
		declared_at: now(),
	})
}

fn parse_object_id(id: &str) -> Result<[u8; 32], String> {
	let hex = id
		.strip_prefix("gd:sha256:")
		.ok_or_else(|| format!("`{id}` is not an object id"))?;
	hex::decode(hex)
		.map_err(|_| format!("`{id}` has a malformed digest"))?
		.try_into()
		.map_err(|_| format!("`{id}` digest must be 32 bytes"))
}

fn random_nonce() -> Vec<u8> {
	let mut nonce = vec![0u8; 16];
	if getrandom::fill(&mut nonce).is_err() {
		panic!("operating system randomness is unavailable");
	}
	nonce
}

fn now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}

fn definition_kind(kind: &str) -> Result<(), String> {
	if matches!(kind, "game" | "loader" | "runtime") {
		Ok(())
	} else {
		Err(format!("unknown definition kind `{kind}` (game, loader, or runtime)"))
	}
}

async fn pull_definition(
	home: &Home,
	from_home: &str,
	kind: &str,
	id: &str,
	api_key: Option<&str>,
) -> Result<serde_json::Value, String> {
	let body = serde_json::json!({ "home_url": from_home, "id": id, "kind": kind }).to_string();
	home.post_json_with_token("/v1/federation/sync-definition", body, api_key)
		.await
}

pub async fn sync_definition(
	home_url: &str,
	from_home: &str,
	kind: &str,
	id: &str,
	api_key: Option<&str>,
) -> Result<(), String> {
	definition_kind(kind)?;
	let home = Home::new(home_url)?;
	let receipt = pull_definition(&home, from_home, kind, id, api_key).await?;
	println!("{kind}: {}", receipt["id"].as_str().unwrap_or(id));
	println!("from: {from_home}");
	Ok(())
}

pub async fn sync_definitions(
	home_url: &str,
	lock_path: &Path,
	default_home: Option<&str>,
	api_key: Option<&str>,
) -> Result<(), String> {
	let text = std::fs::read_to_string(lock_path).map_err(|error| format!("{}: {error}", lock_path.display()))?;
	let lock: DefinitionLock = toml::from_str(&text).map_err(|error| format!("{}: {error}", lock_path.display()))?;
	if lock.definition.is_empty() {
		return Err(format!("{}: no definitions listed", lock_path.display()));
	}
	let home = Home::new(home_url)?;
	let mut pulled = 0;
	for entry in &lock.definition {
		definition_kind(&entry.kind)?;
		let from = entry
			.home
			.as_deref()
			.or(default_home)
			.ok_or_else(|| format!("{}: no home on the entry and no --from given", entry.id))?;
		pull_definition(&home, from, &entry.kind, &entry.id, api_key).await?;
		println!("{}: {} from {}", entry.kind, entry.id, from);
		pulled += 1;
	}
	println!("pulled {pulled} definition(s)");
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parses_a_definition_lock() {
		let lock: DefinitionLock = toml::from_str(
			r#"
[[definition]]
kind = "game"
id = "gd:sha256:aa"
home = "https://definitions.example"

[[definition]]
kind = "loader"
id = "gd:sha256:bb"
"#,
		)
		.expect("parse");
		assert_eq!(lock.definition.len(), 2);
		assert_eq!(lock.definition[0].kind, "game");
		assert_eq!(lock.definition[0].home.as_deref(), Some("https://definitions.example"));
		assert!(lock.definition[1].home.is_none());
	}

	#[test]
	fn rejects_an_unknown_definition_kind() {
		assert!(definition_kind("widget").is_err());
		assert!(definition_kind("game").is_ok());
	}

	#[test]
	fn object_ids_round_trip() {
		let digest = [7u8; 32];
		let id = format!("gd:sha256:{}", hex::encode(digest));
		assert_eq!(parse_object_id(&id).expect("parses"), digest);
		assert!(parse_object_id("not-an-id").is_err());
	}

	#[test]
	fn splits_markdown_into_one_section_per_heading() {
		let sections = markdown_sections("# Fixes\ncrash on launch\n\n## Changes\nnew map\n");
		assert_eq!(sections.len(), 2);
		assert_eq!(sections[0].heading, "Fixes");
		assert_eq!(sections[0].body, "crash on launch");
		assert_eq!(sections[1].heading, "Changes");
		assert_eq!(sections[1].body, "new map");
	}

	#[tokio::test]
	async fn check_validates_a_release_without_uploading() {
		let directory = tempfile::tempdir().expect("tempdir");
		let key_path = directory.path().join("publisher.key");
		keyfile::create(&key_path).expect("key");
		let file = directory.path().join("mod.jar");
		std::fs::write(&file, b"artifact").expect("write");
		check(
			&key_path,
			"https://example.com",
			"p",
			"g",
			&["1.0.0".to_string()],
			"1.0.0",
			"release",
			&file,
			None,
			None,
		)
		.await
		.expect("check");
	}

	#[test]
	fn release_payload_rejects_invalid_preflight_input() {
		let directory = tempfile::tempdir().expect("tempdir");
		let file = directory.path().join("mod.jar");
		std::fs::write(&file, b"artifact").expect("write");
		assert!(release_payload("p", "g", &[], "1.0.0", "release", &file, None, None).is_err());
		assert!(
			release_payload(
				"p",
				"g",
				&["1.0.0".to_string()],
				"1.0.0",
				"release",
				&file,
				None,
				Some("not-a-digest".to_string()),
			)
			.is_err()
		);
		assert!(release_payload("p", "g", &["1.0.0".to_string()], "1.0.0", "release", &file, None, None,).is_ok());
	}

	#[test]
	fn keeps_notes_without_a_heading_under_a_default_section() {
		let sections = markdown_sections("just prose");
		assert_eq!(sections.len(), 1);
		assert_eq!(sections[0].heading, "Notes");
		assert_eq!(sections[0].body, "just prose");
	}
}
