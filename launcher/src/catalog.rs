use std::collections::BTreeSet;

use moraine_model::compatibility::Side;
use moraine_model::dependency::{DependencyKind, TargetKind};
use moraine_model::release::ReleaseObject;
use moraine_model::signed::SignedObject;
use moraine_model::version::{OrderingScheme, VersionCatalog};
use moraine_resolver::{Candidate, Context, Lockfile, Request, resolve};
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
		response
			.bytes()
			.map(|bytes| bytes.to_vec())
			.map_err(|error| error.to_string())
	}
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

pub fn resolve_from_home(fetcher: &dyn HomeFetcher, request: &Request) -> Result<Lockfile, String> {
	let mut queue = vec![request.root_project.clone()];
	let mut visited = BTreeSet::new();
	let mut candidates = Vec::new();

	while let Some(project) = queue.pop() {
		if !visited.insert(project.clone()) {
			continue;
		}
		if visited.len() > MAX_PROJECTS {
			return Err("dependency graph is too large to resolve".to_string());
		}
		let feed = fetcher.get_json(&format!("/v1/projects/{project}/feed?after=0&limit=100"))?;
		let entries = feed.get("entries").and_then(Value::as_array).cloned().unwrap_or_default();
		for entry in entries {
			let kind = entry.get("kind").and_then(Value::as_str).unwrap_or_default();
			if !kind.starts_with("release") {
				continue;
			}
			let Some(object_id) = entry.get("object").and_then(Value::as_str) else {
				continue;
			};
			let Some(hex) = object_id.strip_prefix("gd:sha256:") else {
				continue;
			};
			let wire = fetcher.get_bytes(&format!("/v1/objects/{hex}"))?;
			let signed = SignedObject::<ReleaseObject>::from_bytes(&wire).map_err(|error| error.to_string())?;
			let ReleaseObject::Release(payload) = signed.payload else {
				continue;
			};
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
				release_id: object_id.to_string(),
				human_version: payload.human_version.clone(),
				payload,
			});
		}
	}

	let context = Context {
		game: VersionCatalog::new(OrderingScheme::Semver, Vec::new()),
		loader: None,
		runtime: None,
	};
	resolve(request, &context, &candidates).map_err(|error| error.to_string())
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
		root_project: root_project.to_string(),
		root_predicate: moraine_model::compatibility::Predicate::new(moraine_model::compatibility::Scheme::Any, Vec::new()),
	})
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;

	use moraine_codec::Value as CborValue;
	use moraine_model::Canonical;
	use moraine_model::artifact::Artifact;
	use moraine_model::compatibility::{Compatibility, Predicate, Scheme};
	use moraine_model::release::ReleasePayload;

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

	#[test]
	fn resolves_a_single_release_from_a_home() {
		let payload_bytes = {
			let payload = ReleasePayload {
				protocol: 1,
				project_id: "gd:sha256:root".to_string(),
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
			payload.to_canonical_bytes()
		};
		let object_id = "gd:sha256:release";
		let wire = moraine_codec::encode(&CborValue::map([
			(
				CborValue::text("envelope"),
				CborValue::map([
					(CborValue::text("alg"), CborValue::int(1)),
					(CborValue::text("signatures"), CborValue::array(Vec::new())),
					(CborValue::text("key_ids"), CborValue::array(Vec::new())),
				]),
			),
			(CborValue::text("payload"), CborValue::bytes(payload_bytes)),
		]))
		.expect("wire");

		let mut home = MemoryHome {
			json: BTreeMap::new(),
			bytes: BTreeMap::new(),
		};
		home.json.insert(
			"/v1/projects/gd:sha256:root/feed?after=0&limit=100".to_string(),
			serde_json::json!({
				"project_id": "gd:sha256:root",
				"head_seq": 1,
				"entries": [{ "seq": 1, "kind": "release-published", "object": object_id, "entry": "gd:sha256:e", "declared_at": 0 }],
			}),
		);
		home.bytes.insert("/v1/objects/release".to_string(), wire);

		let request = request_for("gd:sha256:game", "1.20.1", "gd:sha256:root", "client", None, None).expect("request");
		let lockfile = resolve_from_home(&home, &request).expect("resolve");
		assert_eq!(lockfile.releases.len(), 1);
		assert_eq!(lockfile.releases[0].human_version, "1.0.0");
		assert_eq!(lockfile.releases[0].artifact.filename, "example.jar");
	}

	#[test]
	fn rejects_a_non_loopback_http_home() {
		assert!(HttpHome::new("http://example.org", false).is_err());
	}
}
