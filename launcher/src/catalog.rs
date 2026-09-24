use std::collections::BTreeSet;
use std::io::Read;

use moraine_crypto::ObjectKind;
use moraine_model::Canonical;
use moraine_model::compatibility::{Predicate, Side};
use moraine_model::definition::{GameDef, LoaderObject};
use moraine_model::delegation::Delegation;
use moraine_model::dependency::{DependencyKind, TargetKind};
use moraine_model::genesis::GenesisKind;
use moraine_model::release::ReleaseObject;
use moraine_model::signed::SignedObject;
use moraine_model::trust::{RootSet, verify_key_delegation};
use moraine_model::verify::{verify_genesis, verify_object, verify_object_authorized};
use moraine_model::version::{OrderingScheme, VersionCatalog};
use moraine_resolver::{Candidate, Context, LoaderSupport, LockedFeed, Lockfile, Request, resolve};
use serde_json::Value;

const MAX_PROJECTS: usize = 256;

pub trait HomeFetcher {
	fn get_json(&self, path: &str) -> Result<Value, String>;
	fn get_bytes(&self, path: &str) -> Result<Vec<u8>, String>;
}

pub struct HttpHome {
	client: reqwest::blocking::Client,
	base: String,
}

impl HttpHome {
	pub fn new(base: &str, allow_http_local: bool) -> Result<Self, String> {
		validate_url(base, allow_http_local)?;
		let client = reqwest::blocking::Client::builder()
			.timeout(std::time::Duration::from_secs(30))
			.redirect(reqwest::redirect::Policy::none())
			.build()
			.map_err(|error| error.to_string())?;
		Ok(Self {
			client,
			base: base.trim_end_matches('/').to_string(),
		})
	}
}

impl HomeFetcher for HttpHome {
	fn get_json(&self, path: &str) -> Result<Value, String> {
		let bytes = self.get_bytes(path)?;
		serde_json::from_slice(&bytes).map_err(|error| error.to_string())
	}

	fn get_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
		let response = self
			.client
			.get(format!("{}{path}", self.base))
			.send()
			.map_err(|error| error.to_string())?;
		if !response.status().is_success() {
			return Err(format!("home returned {} for {path}", response.status()));
		}
		if let Some(length) = response.content_length()
			&& length > MAX_HOME_RESPONSE_BYTES
		{
			return Err(format!("{path} exceeds the response size limit"));
		}
		read_bounded(response, path)
	}
}

fn read_bounded<R: std::io::Read>(reader: R, path: &str) -> Result<Vec<u8>, String> {
	let mut body = Vec::new();
	reader
		.take(MAX_HOME_RESPONSE_BYTES + 1)
		.read_to_end(&mut body)
		.map_err(|error| error.to_string())?;
	if body.len() as u64 > MAX_HOME_RESPONSE_BYTES {
		return Err(format!("{path} exceeds the response size limit"));
	}
	Ok(body)
}

fn validate_url(value: &str, allow_http_local: bool) -> Result<(), String> {
	let url = reqwest::Url::parse(value).map_err(|error| error.to_string())?;
	match url.scheme() {
		"https" => Ok(()),
		"http" => {
			let host = url.host_str().unwrap_or_default().to_string();
			let loopback = host == "localhost"
				|| host
					.parse::<std::net::IpAddr>()
					.map(|address| address.is_loopback())
					.unwrap_or(false);
			if allow_http_local && loopback {
				Ok(())
			} else {
				Err("http is only allowed for loopback when explicitly enabled".to_string())
			}
		}
		_ => Err("home url must use https".to_string()),
	}
}

const MAX_HOME_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
const FEED_PAGE_LIMIT: i64 = 100;
const MAX_FEED_PAGES: usize = 200;

pub fn resolve_from_home(
	fetcher: &dyn HomeFetcher,
	request: &Request,
	previous: Option<&Lockfile>,
) -> Result<Lockfile, String> {
	let game_catalog = game_definition(fetcher, &request.game_id)
		.ok()
		.and_then(|game| game.definition.catalog())
		.unwrap_or_else(|| VersionCatalog::new(OrderingScheme::Semver, Vec::new()));
	let (loader_catalog, loader_support, loader_game_versions) = match request.loader_id.as_deref() {
		Some(loader_id) => loader_context(fetcher, loader_id),
		None => (None, Vec::new(), None),
	};
	let mut queue = vec![request.root_project.clone()];
	let mut visited = BTreeSet::new();
	let mut candidates = Vec::new();
	let mut feeds: Vec<LockedFeed> = Vec::new();

	while let Some(project) = queue.pop() {
		if !visited.insert(project.clone()) {
			continue;
		}
		if visited.len() > MAX_PROJECTS {
			return Err("dependency graph is too large to resolve".to_string());
		}
		let trust = project_trust(fetcher, &project)?;
		let (entries, head_seq) = feed_entries(fetcher, &project)?;
		if let Some(previous) = previous
			&& let Some(seen) = previous.feeds.iter().find(|feed| feed.project_id == project)
			&& head_seq < seen.head_seq
		{
			return Err(format!(
				"{project} went backwards from sequence {} to {head_seq}",
				seen.head_seq
			));
		}
		feeds.push(LockedFeed {
			project_id: project.clone(),
			head_seq,
		});
		let delegations = project_delegations(fetcher, &entries, &trust.root)?;
		for entry in &entries {
			let kind = entry.get("kind").and_then(Value::as_str).unwrap_or_default();
			if !kind.starts_with("release") {
				continue;
			}
			let Some(object_id) = entry.get("object").and_then(Value::as_str) else {
				continue;
			};
			let wire = fetch_object(fetcher, object_id)?;
			let object = verify_object_authorized(ObjectKind::Release, &wire, &trust.root, &delegations, unix_now())
				.map_err(|error| format!("{object_id} did not verify: {error}"))?;
			if object.id != object_id {
				return Err(format!("the home served {object_id} but its bytes are {}", object.id));
			}
			let Ok(ReleaseObject::Release(payload)) = ReleaseObject::from_canonical_bytes(&object.payload_bytes) else {
				continue;
			};
			if payload.project_id != project {
				return Err(format!("{object_id} belongs to a different project"));
			}
			for dependency in &payload.dependencies {
				if dependency.target_kind == TargetKind::Project
					&& dependency.kind == DependencyKind::Required
					&& dependency.game_id == request.game_id
				{
					queue.push(dependency.target_id.clone());
				}
			}
			candidates.push(Candidate {
				project_id: payload.project_id.clone(),
				release_id: object.id.clone(),
				human_version: payload.human_version.clone(),
				payload,
			});
		}
	}

	let context = Context {
		game: game_catalog,
		loader: loader_catalog,
		runtime: None,
		loader_support,
		loader_game_versions,
	};
	let mut lockfile = resolve(request, &context, &candidates).map_err(|error| error.to_string())?;
	lockfile.feeds = feeds;
	Ok(lockfile)
}

pub struct GameDefinition {
	pub id: String,
	pub definition: GameDef,
	pub object_bytes: Vec<u8>,
}

pub fn game_definition(fetcher: &dyn HomeFetcher, game_id: &str) -> Result<GameDefinition, String> {
	let view = fetcher.get_json(&format!("/v1/games/{game_id}"))?;
	let genesis_id = view
		.get("genesis")
		.and_then(Value::as_str)
		.ok_or_else(|| format!("the home did not describe game {game_id}"))?;
	let current_id = view
		.get("current")
		.and_then(Value::as_str)
		.ok_or_else(|| format!("the home has no definition for game {game_id}"))?;
	let genesis_wire = fetch_object(fetcher, genesis_id)?;
	let (root, object) = verify_genesis(&genesis_wire).map_err(|error| format!("{genesis_id} did not verify: {error}"))?;
	if root.genesis_kind() != GenesisKind::Game || object.id != genesis_id {
		return Err(format!("{genesis_id} is not that game's genesis"));
	}
	let definition_wire = fetch_object(fetcher, current_id)?;
	let verified = verify_object(ObjectKind::GameDef, &definition_wire, &root)
		.map_err(|error| format!("{current_id} did not verify: {error}"))?;
	if verified.id != current_id {
		return Err(format!("the home served {current_id} but its bytes are {}", verified.id));
	}
	let definition =
		GameDef::from_canonical_bytes(&verified.payload_bytes).map_err(|error| format!("{current_id}: {error}"))?;
	if definition.game_id != game_id {
		return Err(format!("`{game_id}` does not match the game definition it resolves to"));
	}
	Ok(GameDefinition {
		id: verified.id,
		definition,
		object_bytes: definition_wire,
	})
}

struct LoaderTrust {
	root: RootSet,
	catalog: Option<VersionCatalog>,
	game_versions: Option<Predicate>,
}

fn loader_context(
	fetcher: &dyn HomeFetcher,
	loader_id: &str,
) -> (Option<VersionCatalog>, Vec<LoaderSupport>, Option<Predicate>) {
	let Ok(trust) = loader_trust(fetcher, loader_id) else {
		return (None, Vec::new(), None);
	};
	let support = loader_support(fetcher, loader_id, &trust.root).unwrap_or_default();
	(trust.catalog, support, trust.game_versions)
}

fn loader_trust(fetcher: &dyn HomeFetcher, loader_id: &str) -> Result<LoaderTrust, String> {
	let view = fetcher.get_json(&format!("/v1/loaders/{loader_id}"))?;
	let genesis_id = view
		.get("genesis")
		.and_then(Value::as_str)
		.ok_or_else(|| format!("the home did not describe loader {loader_id}"))?;
	let current_id = view
		.get("current")
		.and_then(Value::as_str)
		.ok_or_else(|| format!("the home has no definition for loader {loader_id}"))?;
	let genesis_wire = fetch_object(fetcher, genesis_id)?;
	let (root, object) = verify_genesis(&genesis_wire).map_err(|error| format!("{genesis_id} did not verify: {error}"))?;
	if root.genesis_kind() != GenesisKind::Loader || object.id != genesis_id {
		return Err(format!("{genesis_id} is not that loader's genesis"));
	}
	let definition_wire = fetch_object(fetcher, current_id)?;
	let verified = verify_object(ObjectKind::LoaderDef, &definition_wire, &root)
		.map_err(|error| format!("{current_id} did not verify: {error}"))?;
	let (catalog, game_versions) = match LoaderObject::from_canonical_bytes(&verified.payload_bytes) {
		Ok(LoaderObject::Definition(definition)) => (definition.catalog(), definition.game_versions),
		_ => (None, None),
	};
	Ok(LoaderTrust {
		root,
		catalog,
		game_versions,
	})
}

fn loader_support(fetcher: &dyn HomeFetcher, loader_id: &str, root: &RootSet) -> Result<Vec<LoaderSupport>, String> {
	let list = fetcher.get_json(&format!("/v1/loaders/{loader_id}/releases"))?;
	let Some(items) = list.as_array() else {
		return Ok(Vec::new());
	};
	let mut support = Vec::new();
	for item in items {
		let Some(release_id) = item.get("release").and_then(Value::as_str) else {
			continue;
		};
		let Ok(wire) = fetch_object(fetcher, release_id) else {
			continue;
		};
		let Ok(verified) = verify_object(ObjectKind::LoaderDef, &wire, root) else {
			continue;
		};
		let Ok(LoaderObject::Release(release)) = LoaderObject::from_canonical_bytes(&verified.payload_bytes) else {
			continue;
		};
		support.push(LoaderSupport {
			version: release.version_id,
			game_version_predicate: release.game_version_predicate,
		});
	}
	Ok(support)
}

struct ProjectTrust {
	root: RootSet,
}

fn project_trust(fetcher: &dyn HomeFetcher, project: &str) -> Result<ProjectTrust, String> {
	let summary = fetcher.get_json(&format!("/v1/projects/{project}"))?;
	let genesis_id = summary
		.get("genesis")
		.and_then(Value::as_str)
		.ok_or_else(|| format!("the home did not describe {project}"))?;
	let wire = fetch_object(fetcher, genesis_id)?;
	let (root, object) = verify_genesis(&wire).map_err(|error| format!("{genesis_id} did not verify: {error}"))?;
	if root.genesis_kind() != GenesisKind::Project {
		return Err(format!("{genesis_id} is not a project genesis"));
	}
	if object.id != genesis_id || object.id != project {
		return Err(format!("{project} does not match its genesis"));
	}
	Ok(ProjectTrust { root })
}

fn feed_entries(fetcher: &dyn HomeFetcher, project: &str) -> Result<(Vec<Value>, i64), String> {
	let mut entries = Vec::new();
	let mut after = 0i64;
	let mut head = 0i64;
	for _ in 0..MAX_FEED_PAGES {
		let feed = fetcher.get_json(&format!("/v1/projects/{project}/feed?after={after}&limit={FEED_PAGE_LIMIT}"))?;
		head = feed.get("head_seq").and_then(Value::as_i64).unwrap_or(after);
		let page = feed.get("entries").and_then(Value::as_array).cloned().unwrap_or_default();
		if page.is_empty() {
			break;
		}
		let last = page
			.iter()
			.filter_map(|entry| entry.get("seq").and_then(Value::as_i64))
			.next_back();
		entries.extend(page);
		match last {
			Some(seq) if seq > after && seq < head => after = seq,
			_ => break,
		}
	}
	Ok((entries, head))
}

fn project_delegations(
	fetcher: &dyn HomeFetcher,
	entries: &[Value],
	root: &RootSet,
) -> Result<Vec<SignedObject<Delegation>>, String> {
	let mut delegations = Vec::new();
	for entry in entries {
		let kind = entry.get("kind").and_then(Value::as_str).unwrap_or_default();
		if !matches!(kind, "key-changed" | "migration" | "recovery") {
			continue;
		}
		let Some(object_id) = entry.get("object").and_then(Value::as_str) else {
			continue;
		};
		let wire = fetch_object(fetcher, object_id)?;
		let signed = SignedObject::<Delegation>::from_bytes(&wire).map_err(|error| error.to_string())?;
		if matches!(signed.payload, Delegation::Key(_)) {
			verify_key_delegation(&signed, root).map_err(|error| format!("{object_id} did not verify: {error}"))?;
			delegations.push(signed);
		}
	}
	Ok(delegations)
}

fn fetch_object(fetcher: &dyn HomeFetcher, object_id: &str) -> Result<Vec<u8>, String> {
	let hex = object_id
		.strip_prefix("gd:sha256:")
		.ok_or_else(|| format!("`{object_id}` is not an object id"))?;
	fetcher.get_bytes(&format!("/v1/objects/{hex}"))
}

fn unix_now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|elapsed| elapsed.as_secs() as i64)
		.unwrap_or(0)
}

pub fn request_for(
	game_id: &str,
	game_version: &str,
	root_project: &str,
	side: &str,
	loader: Option<(String, Option<String>)>,
	runtime: Option<(String, Option<String>)>,
) -> Result<Request, String> {
	Ok(Request {
		game_id: game_id.to_string(),
		game_version: game_version.to_string(),
		loader_id: loader.as_ref().map(|(id, _)| id.clone()),
		loader_version: loader.as_ref().and_then(|(_, version)| version.clone()),
		runtime_id: runtime.as_ref().map(|(id, _)| id.clone()),
		runtime_version: runtime.as_ref().and_then(|(_, version)| version.clone()),
		side: Side::parse(side).ok_or_else(|| format!("unknown side `{side}`"))?,
		os: Some(std::env::consts::OS.to_string()),
		arch: Some(std::env::consts::ARCH.to_string()),
		root_project: root_project.to_string(),
		root_predicate: moraine_model::compatibility::Predicate::new(moraine_model::compatibility::Scheme::Any, Vec::new()),
	})
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;

	use moraine_crypto::{ObjectKind, SigningKey, object_id};
	use moraine_model::artifact::Artifact;
	use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
	use moraine_model::feed::FeedEntry;
	use moraine_model::genesis::{Genesis, RootKey};
	use moraine_model::release::ReleasePayload;
	use moraine_model::signed::sign_payload;

	use super::*;

	struct MemoryHome {
		json: BTreeMap<String, Value>,
		bytes: BTreeMap<String, Vec<u8>>,
	}

	impl HomeFetcher for MemoryHome {
		fn get_json(&self, path: &str) -> Result<Value, String> {
			self.json.get(path).cloned().ok_or_else(|| format!("no json for {path}"))
		}

		fn get_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
			self.bytes.get(path).cloned().ok_or_else(|| format!("no bytes for {path}"))
		}
	}

	fn signed_home(forged: bool) -> (MemoryHome, String) {
		let signer = SigningKey::from_seed(&[7u8; 32]);
		let genesis = Genesis {
			protocol: 1,
			kind: GenesisKind::Project,
			nonce: vec![0x33; 16],
			roots: vec![RootKey::from_public_key(signer.verifying_key().to_bytes().to_vec()).expect("root")],
			threshold: 1,
			authorized_kinds: vec![
				"release".to_string(),
				"feed-entry".to_string(),
				"delegation".to_string(),
				"profile".to_string(),
				"advisory".to_string(),
			],
			home_hint: None,
			contacts: None,
			created_at: 1_760_000_000,
		};
		let genesis_signed = sign_payload(ObjectKind::Genesis, &genesis, &[&signer]);
		let genesis_id = genesis_signed.id(ObjectKind::Genesis);

		let release = ReleasePayload {
			protocol: 1,
			project_id: genesis_id.clone(),
			game_id: "gd:sha256:game".to_string(),
			release_nonce: vec![0x11; 16],
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
		let intruder = SigningKey::from_seed(&[8u8; 32]);
		let release_signer = if forged { &intruder } else { &signer };
		let release_signed = sign_payload(ObjectKind::Release, &release, &[release_signer]);
		let release_id = release_signed.id(ObjectKind::Release);

		let entry = FeedEntry {
			protocol: 1,
			project_id: genesis_id.clone(),
			sequence: 1,
			previous: None,
			kind: "release-published".to_string(),
			object_digest: object_id(ObjectKind::Release, &release_signed.payload_bytes).to_vec(),
			declared_at: 1_760_000_000,
		};
		let entry_signed = sign_payload(ObjectKind::FeedEntry, &entry, &[&signer]);
		let entry_id = entry_signed.id(ObjectKind::FeedEntry);

		let mut home = MemoryHome {
			json: BTreeMap::new(),
			bytes: BTreeMap::new(),
		};
		home.json.insert(
			format!("/v1/projects/{genesis_id}"),
			serde_json::json!({ "genesis": genesis_id }),
		);
		home.json.insert(
			format!("/v1/projects/{genesis_id}/feed?after=0&limit=100"),
			serde_json::json!({
				"project_id": genesis_id,
				"head_seq": 1,
				"entries": [{ "seq": 1, "kind": "release-published", "object": release_id, "entry": entry_id, "declared_at": 0 }],
			}),
		);
		for (id, wire) in [
			(genesis_id.clone(), genesis_signed.wire_bytes()),
			(release_id, release_signed.wire_bytes()),
		] {
			let hex = id.strip_prefix("gd:sha256:").expect("object id").to_string();
			home.bytes.insert(format!("/v1/objects/{hex}"), wire);
		}
		(home, genesis_id)
	}

	#[test]
	fn resolves_a_single_release_from_a_home() {
		let (home, project_id) = signed_home(false);
		let request = request_for("gd:sha256:game", "1.20.1", &project_id, "client", None, None).expect("request");
		let lockfile = resolve_from_home(&home, &request, None).expect("resolve");
		assert_eq!(lockfile.releases.len(), 1);
		assert_eq!(lockfile.releases[0].human_version, "1.0.0");
		assert_eq!(lockfile.releases[0].artifact.filename, "example.jar");
		assert_eq!(lockfile.feeds.len(), 1);
		assert_eq!(lockfile.feeds[0].head_seq, 1);
	}

	#[test]
	fn rejects_a_release_the_home_did_not_authorize() {
		let (home, project_id) = signed_home(true);
		let request = request_for("gd:sha256:game", "1.20.1", &project_id, "client", None, None).expect("request");
		assert!(resolve_from_home(&home, &request, None).is_err());
	}

	#[test]
	fn rejects_a_feed_that_went_backwards() {
		let (home, project_id) = signed_home(false);
		let request = request_for("gd:sha256:game", "1.20.1", &project_id, "client", None, None).expect("request");
		let mut previous = resolve_from_home(&home, &request, None).expect("resolve");
		previous.feeds[0].head_seq = 5;

		let error = resolve_from_home(&home, &request, Some(&previous)).expect_err("rollback");

		assert!(error.contains("went backwards"), "{error}");
	}

	#[test]
	fn bounds_what_a_home_can_return() {
		assert!(read_bounded(std::io::Cursor::new(b"small".to_vec()), "/x").is_ok());
		let oversized = std::io::Cursor::new(vec![0u8; (MAX_HOME_RESPONSE_BYTES + 1) as usize]);
		assert!(read_bounded(oversized, "/x").is_err());
	}

	#[test]
	fn rejects_a_non_loopback_http_home() {
		assert!(HttpHome::new("http://example.org", false).is_err());
	}

	fn signed_game() -> (MemoryHome, String) {
		use moraine_model::definition::{Category, GameDef, Tag, VersionSyntax};

		let signer = SigningKey::from_seed(&[9u8; 32]);
		let genesis = Genesis {
			protocol: 1,
			kind: GenesisKind::Game,
			nonce: vec![0x44; 16],
			roots: vec![RootKey::from_public_key(signer.verifying_key().to_bytes().to_vec()).expect("root")],
			threshold: 1,
			authorized_kinds: vec!["delegation".to_string(), "game-def".to_string()],
			home_hint: None,
			contacts: None,
			created_at: 1_760_000_000,
		};
		let genesis_signed = sign_payload(ObjectKind::Genesis, &genesis, &[&signer]);
		let game_id = genesis_signed.id(ObjectKind::Genesis);

		let definition = GameDef {
			protocol: 1,
			game_id: game_id.clone(),
			display_name: "Example".to_string(),
			version_syntax: VersionSyntax {
				kind: "semver".to_string(),
				pattern: None,
			},
			version_ordering: "semver".to_string(),
			version_catalog: Vec::new(),
			loaders_allowed: true,
			loader_authorities: Vec::new(),
			categories: vec![Category {
				id: "gameplay".to_string(),
				label: "Gameplay".to_string(),
				parent: None,
			}],
			tags: vec![Tag {
				id: "performance".to_string(),
				label: "Performance".to_string(),
			}],
			metadata_extractor: Some("minecraft/fabric-json".to_string()),
			install_adapter: Some("minecraft/default".to_string()),
			declared_time: 1_760_000_000,
		};
		let definition_signed = sign_payload(ObjectKind::GameDef, &definition, &[&signer]);
		let definition_id = definition_signed.id(ObjectKind::GameDef);

		let mut home = MemoryHome {
			json: BTreeMap::new(),
			bytes: BTreeMap::new(),
		};
		home.json.insert(
			format!("/v1/games/{game_id}"),
			serde_json::json!({ "genesis": game_id, "current": definition_id }),
		);
		for (id, wire) in [
			(game_id.clone(), genesis_signed.wire_bytes()),
			(definition_id, definition_signed.wire_bytes()),
		] {
			let hex = id.strip_prefix("gd:sha256:").expect("object id").to_string();
			home.bytes.insert(format!("/v1/objects/{hex}"), wire);
		}
		(home, game_id)
	}

	#[test]
	fn reads_the_adapter_and_extractor_a_game_definition_declares() {
		let (home, game_id) = signed_game();
		let game = game_definition(&home, &game_id).expect("definition");
		assert_eq!(game.definition.install_adapter.as_deref(), Some("minecraft/default"));
		assert_eq!(game.definition.metadata_extractor.as_deref(), Some("minecraft/fabric-json"));
	}

	#[test]
	fn refuses_a_game_definition_that_does_not_match_its_id() {
		let (home, _) = signed_game();
		assert!(game_definition(&home, "gd:sha256:nothing").is_err());
	}
}
