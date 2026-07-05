use std::path::Path;

use moraine_crypto::ObjectKind;
use moraine_model::artifact::Artifact;
use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::release::ReleasePayload;
use moraine_model::signed::sign_payload;

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
		authorized_kinds: vec!["delegation".to_string(), "release".to_string(), "profile".to_string()],
		home_hint: home_hint.or_else(|| Some(home_url.to_string())),
		contacts: None,
		created_at: now(),
	};
	let signed = sign_payload(ObjectKind::Genesis, &genesis, &[&key]);
	let home = Home::new(home_url)?;
	let receipt = home.post_wire("/v1/projects", signed.wire_bytes()).await?;
	println!("project_id: {}", receipt["project_id"].as_str().unwrap_or("?"));
	println!("home: {}", home.base());
	Ok(())
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
) -> Result<(), String> {
	if game_versions.is_empty() {
		return Err("at least one --game-version is required".to_string());
	}
	let key = keyfile::load(key_path)?;
	let artifact_file = artifacts::describe(file)?;
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
		changelog_digest: None,
		license_expression: None,
		rights: None,
		sbom_digest: None,
		minimum_verifier_version: 1,
		critical_extensions: Vec::new(),
	};
	let signed = sign_payload(ObjectKind::Release, &release, &[&key]);
	let home = Home::new(home_url)?;
	let receipt = home
		.post_wire(&format!("/v1/projects/{project_id}/objects/release"), signed.wire_bytes())
		.await?;
	println!("release: {}", receipt["id"].as_str().unwrap_or("?"));
	println!("artifact: sha256:{}", hex::encode(artifact_file.digest));
	Ok(())
}

pub async fn publish(key_path: &Path, home_url: &str, project_id: &str, object_id: &str) -> Result<(), String> {
	let key = keyfile::load(key_path)?;
	let home = Home::new(home_url)?;
	let project = home.get_json(&format!("/v1/projects/{project_id}")).await?;
	let head_seq = project["head_seq"].as_i64().unwrap_or(0);
	let previous = match project["head_entry"].as_str() {
		Some(entry) => Some(parse_object_id(entry)?.to_vec()),
		None => None,
	};
	let entry = FeedEntry {
		protocol: 1,
		project_id: project_id.to_string(),
		sequence: (head_seq + 1) as u64,
		previous,
		kind: "release-published".to_string(),
		object_digest: parse_object_id(object_id)?.to_vec(),
		declared_at: now(),
	};
	let signed = sign_payload(ObjectKind::FeedEntry, &entry, &[&key]);
	let receipt = home
		.post_wire(&format!("/v1/projects/{project_id}/feed"), signed.wire_bytes())
		.await?;
	println!("seq: {}", receipt["seq"].as_i64().unwrap_or(0));
	println!("entry: {}", receipt["entry"].as_str().unwrap_or("?"));
	Ok(())
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn object_ids_round_trip() {
		let digest = [7u8; 32];
		let id = format!("gd:sha256:{}", hex::encode(digest));
		assert_eq!(parse_object_id(&id).expect("parses"), digest);
		assert!(parse_object_id("not-an-id").is_err());
	}
}
