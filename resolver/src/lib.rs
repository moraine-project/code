use std::collections::BTreeMap;

use moraine_model::compatibility::{Predicate, PredicateResult, Scheme, Side};
use moraine_model::dependency::{Dependency, DependencyKind, TargetKind};
use moraine_model::release::ReleasePayload;
use moraine_model::version::{OrderingScheme, VersionCatalog};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Request {
	pub game_id: String,
	pub game_version: String,
	pub loader_id: Option<String>,
	pub loader_version: Option<String>,
	pub runtime_id: Option<String>,
	pub runtime_version: Option<String>,
	pub side: Side,
	pub root_project: String,
	pub root_predicate: Predicate,
}

#[derive(Debug, Clone)]
pub struct Context {
	pub game: VersionCatalog,
	pub loader: Option<VersionCatalog>,
	pub runtime: Option<VersionCatalog>,
}

#[derive(Debug, Clone)]
pub struct Candidate {
	pub project_id: String,
	pub release_id: String,
	pub human_version: String,
	pub payload: ReleasePayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedArtifact {
	pub digest: String,
	pub size: u64,
	pub filename: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedRelease {
	pub project_id: String,
	pub release_id: String,
	pub human_version: String,
	pub artifact: LockedArtifact,
	pub dependencies: Vec<LockedDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedDependency {
	pub target_kind: String,
	pub target_id: String,
	pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
	pub lockfile_version: u32,
	pub trust_policy_version: u32,
	pub game_id: String,
	pub game_version: String,
	pub loader_id: Option<String>,
	pub loader_version: Option<String>,
	pub runtime_id: Option<String>,
	pub runtime_version: Option<String>,
	pub side: String,
	pub releases: Vec<LockedRelease>,
	#[serde(default)]
	pub feeds: Vec<LockedFeed>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedFeed {
	pub project_id: String,
	pub head_seq: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
	NoCandidate {
		project_id: String,
		detail: String,
	},
	Conflict {
		project_id: String,
		detail: String,
	},
	Cycle {
		path: String,
	},
	CrossGame {
		project_id: String,
		game_id: String,
	},
	MissingArtifact {
		project_id: String,
	},
	TooDeep,
}

impl std::fmt::Display for ResolveError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::NoCandidate { project_id, detail } => write!(f, "no candidate for `{project_id}`: {detail}"),
			Self::Conflict { project_id, detail } => write!(f, "conflict on `{project_id}`: {detail}"),
			Self::Cycle { path } => write!(f, "dependency cycle: {path}"),
			Self::CrossGame { project_id, game_id } => {
				write!(f, "`{project_id}` targets game `{game_id}`, not this one")
			}
			Self::MissingArtifact { project_id } => write!(f, "`{project_id}` has no primary artifact"),
			Self::TooDeep => f.write_str("dependency graph is too deep to resolve"),
		}
	}
}

impl std::error::Error for ResolveError {}

const MAX_DEPTH: usize = 64;

pub fn resolve<'a>(request: &Request, context: &Context, candidates: &'a [Candidate]) -> Result<Lockfile, ResolveError> {
	let mut by_project: BTreeMap<&'a str, Vec<&'a Candidate>> = BTreeMap::new();
	for candidate in candidates {
		by_project.entry(candidate.project_id.as_str()).or_default().push(candidate);
	}
	for group in by_project.values_mut() {
		group.sort_by(|a, b| compare_versions(&b.human_version, &a.human_version));
	}

	let mut selected: BTreeMap<String, &'a Candidate> = BTreeMap::new();
	let mut path = Vec::new();
	solve(
		request,
		context,
		&by_project,
		vec![(request.root_project.clone(), request.root_predicate.clone())],
		&mut selected,
		&mut path,
	)?;

	let mut releases = Vec::new();
	for (project_id, candidate) in &selected {
		let artifact = primary_artifact(candidate).ok_or_else(|| ResolveError::MissingArtifact {
			project_id: project_id.clone(),
		})?;
		releases.push(LockedRelease {
			project_id: project_id.clone(),
			release_id: candidate.release_id.clone(),
			human_version: candidate.human_version.clone(),
			artifact: LockedArtifact {
				digest: format!("sha256:{}", hex::encode(&artifact.digest)),
				size: artifact.size,
				filename: artifact.filename.clone(),
			},
			dependencies: candidate
				.payload
				.dependencies
				.iter()
				.map(|dependency| LockedDependency {
					target_kind: dependency.target_kind.as_str().to_string(),
					target_id: dependency.target_id.clone(),
					kind: dependency.kind.as_str().to_string(),
				})
				.collect(),
		});
	}
	releases.sort_by(|a, b| a.project_id.cmp(&b.project_id));

	Ok(Lockfile {
		lockfile_version: 1,
		trust_policy_version: 1,
		game_id: request.game_id.clone(),
		game_version: request.game_version.clone(),
		loader_id: request.loader_id.clone(),
		loader_version: request.loader_version.clone(),
		runtime_id: request.runtime_id.clone(),
		runtime_version: request.runtime_version.clone(),
		side: request.side.as_str().to_string(),
		releases,
		feeds: Vec::new(),
	})
}

fn solve<'a>(
	request: &Request,
	context: &Context,
	by_project: &BTreeMap<&'a str, Vec<&'a Candidate>>,
	mut queue: Vec<(String, Predicate)>,
	selected: &mut BTreeMap<String, &'a Candidate>,
	path: &mut Vec<String>,
) -> Result<(), ResolveError> {
	if queue.is_empty() {
		return Ok(());
	}
	if path.len() > MAX_DEPTH {
		return Err(ResolveError::TooDeep);
	}
	let (project_id, predicate) = queue.remove(0);

	if path.contains(&project_id) {
		let mut chain = path.clone();
		chain.push(project_id);
		return Err(ResolveError::Cycle {
			path: chain.join(" -> "),
		});
	}

	if let Some(existing) = selected.get(&project_id) {
		if satisfies(&predicate, &existing.human_version) {
			return solve(request, context, by_project, queue, selected, path);
		}
		return Err(ResolveError::Conflict {
			project_id,
			detail: format!(
				"selected {} does not satisfy an additional constraint",
				existing.human_version
			),
		});
	}

	let candidates = by_project.get(project_id.as_str()).cloned().unwrap_or_default();
	for candidate in candidates {
		if !compatible(request, context, candidate) {
			continue;
		}
		if !satisfies(&predicate, &candidate.human_version) {
			continue;
		}
		let mut dependencies = Vec::new();
		if collect_dependencies(request, context, candidate, &mut dependencies).is_err() {
			continue;
		}

		selected.insert(project_id.clone(), candidate);
		path.push(project_id.clone());
		let mut next = dependencies;
		next.extend(queue.clone());
		match solve(request, context, by_project, next, selected, path) {
			Ok(()) => return Ok(()),
			Err(ResolveError::Conflict { .. } | ResolveError::NoCandidate { .. }) => {
				selected.remove(&project_id);
				path.pop();
			}
			Err(error) => return Err(error),
		}
	}

	Err(ResolveError::NoCandidate {
		project_id,
		detail: "no release matches the request, the predicate, and its dependencies".to_string(),
	})
}

fn collect_dependencies(
	request: &Request,
	context: &Context,
	candidate: &Candidate,
	out: &mut Vec<(String, Predicate)>,
) -> Result<(), ResolveError> {
	for dependency in &candidate.payload.dependencies {
		if !applies_to_side(dependency, request.side) {
			continue;
		}
		match dependency.kind {
			DependencyKind::Required => {}
			DependencyKind::Incompatible => {
				return Err(ResolveError::Conflict {
					project_id: candidate.project_id.clone(),
					detail: "a required incompatible dependency is not auto-resolved".to_string(),
				});
			}
			DependencyKind::Optional | DependencyKind::Embedded | DependencyKind::Recommended => continue,
		}
		match dependency.target_kind {
			TargetKind::Project => {
				if dependency.game_id != request.game_id {
					return Err(ResolveError::CrossGame {
						project_id: dependency.target_id.clone(),
						game_id: dependency.game_id.clone(),
					});
				}
				out.push((dependency.target_id.clone(), dependency.predicate.clone()));
			}
			TargetKind::Loader | TargetKind::Runtime => {
				if !context_dependency_satisfied(request, context, dependency) {
					return Err(ResolveError::Conflict {
						project_id: candidate.project_id.clone(),
						detail: format!(
							"{} dependency is not satisfied by the request",
							dependency.target_kind.as_str()
						),
					});
				}
			}
		}
	}
	Ok(())
}

fn context_dependency_satisfied(request: &Request, context: &Context, dependency: &Dependency) -> bool {
	match dependency.target_kind {
		TargetKind::Loader => {
			request.loader_id.as_deref() == Some(dependency.target_id.as_str())
				&& request
					.loader_version
					.as_deref()
					.is_some_and(|version| satisfies_catalog(&dependency.predicate, version, context.loader.as_ref()))
		}
		TargetKind::Runtime => {
			request.runtime_id.as_deref() == Some(dependency.target_id.as_str())
				&& request
					.runtime_version
					.as_deref()
					.is_some_and(|version| satisfies_catalog(&dependency.predicate, version, context.runtime.as_ref()))
		}
		TargetKind::Project => true,
	}
}

fn compatible(request: &Request, context: &Context, candidate: &Candidate) -> bool {
	candidate.payload.game_id == request.game_id
		&& candidate.payload.compatibility.iter().any(|entry| {
			let game_ok = satisfies_catalog(&entry.game_version_predicate, &request.game_version, Some(&context.game));
			let side_ok = entry.side == Side::Both || entry.side == request.side;
			let loader_ok = match (&entry.loader_id, &request.loader_id) {
				(None, None) => true,
				(Some(want), Some(have)) if want == have => {
					entry.loader_version_predicate.as_ref().is_none_or(|predicate| {
						request
							.loader_version
							.as_deref()
							.is_some_and(|version| satisfies_catalog(predicate, version, context.loader.as_ref()))
					})
				}
				_ => false,
			};
			let runtime_ok = entry.runtime_predicate.as_ref().is_none_or(|predicate| {
				request
					.runtime_version
					.as_deref()
					.is_some_and(|version| satisfies_catalog(predicate, version, context.runtime.as_ref()))
			});
			game_ok && side_ok && loader_ok && runtime_ok
		}) && entry_side_supported(candidate, request.side)
}

fn entry_side_supported(candidate: &Candidate, side: Side) -> bool {
	candidate
		.payload
		.compatibility
		.iter()
		.any(|entry| entry.side == Side::Both || entry.side == side)
}

fn applies_to_side(dependency: &Dependency, side: Side) -> bool {
	dependency.applies_to == Side::Both || dependency.applies_to == side
}

fn satisfies(predicate: &Predicate, version: &str) -> bool {
	satisfies_catalog(predicate, version, None)
}

fn satisfies_catalog(predicate: &Predicate, version: &str, catalog: Option<&VersionCatalog>) -> bool {
	match predicate.scheme() {
		Some(Scheme::Any) => true,
		Some(Scheme::Exact) | Some(Scheme::Set) => predicate.values.iter().any(|candidate| candidate == version),
		Some(Scheme::Semver) => {
			let catalog = VersionCatalog::new(OrderingScheme::Semver, Vec::new());
			catalog.evaluate(predicate, version) == PredicateResult::Satisfied
		}
		Some(Scheme::OrderedList) | Some(Scheme::Calendar) => {
			catalog.is_some_and(|catalog| catalog.evaluate(predicate, version) == PredicateResult::Satisfied)
		}
		None => false,
	}
}

fn primary_artifact(candidate: &Candidate) -> Option<&moraine_model::artifact::Artifact> {
	candidate
		.payload
		.artifacts
		.iter()
		.find(|artifact| artifact.is_primary)
		.or_else(|| candidate.payload.artifacts.first())
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
	let catalog = VersionCatalog::new(OrderingScheme::Semver, Vec::new());
	catalog.compare(a, b).unwrap_or_else(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
	use moraine_model::artifact::Artifact;
	use moraine_model::compatibility::Compatibility;
	use moraine_model::dependency::{Dependency, DependencyKind, TargetKind};
	use moraine_model::release::ReleasePayload;

	use super::*;

	fn candidate(project: &str, version: &str, dependencies: Vec<Dependency>) -> Candidate {
		Candidate {
			project_id: project.to_string(),
			release_id: format!("gd:sha256:{project}-{version}"),
			human_version: version.to_string(),
			payload: ReleasePayload {
				protocol: 1,
				project_id: project.to_string(),
				game_id: "gd:sha256:game".to_string(),
				release_nonce: vec![0x11; 16],
				human_version: version.to_string(),
				channel: "release".to_string(),
				kind: "mod".to_string(),
				declared_time: 1_760_000_000,
				compatibility: vec![Compatibility {
					game_version_predicate: Predicate::new(Scheme::Semver, vec![">=1.20.0".to_string()]),
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
					filename: format!("{project}.jar"),
					is_primary: true,
					os_predicate: None,
					arch_predicate: None,
				}],
				dependencies,
				source_reference: None,
				changelog_digest: None,
				license_expression: None,
				rights: None,
				sbom_digest: None,
				minimum_verifier_version: 1,
				critical_extensions: Vec::new(),
			},
		}
	}

	fn require(project: &str, predicate: Predicate) -> Dependency {
		Dependency {
			target_kind: TargetKind::Project,
			target_id: project.to_string(),
			game_id: "gd:sha256:game".to_string(),
			predicate,
			kind: DependencyKind::Required,
			applies_to: Side::Both,
		}
	}

	fn request() -> Request {
		Request {
			game_id: "gd:sha256:game".to_string(),
			game_version: "1.20.1".to_string(),
			loader_id: None,
			loader_version: None,
			runtime_id: None,
			runtime_version: None,
			side: Side::Client,
			root_project: "root".to_string(),
			root_predicate: Predicate::new(Scheme::Any, Vec::new()),
		}
	}

	fn context() -> Context {
		Context {
			game: VersionCatalog::new(OrderingScheme::Semver, Vec::new()),
			loader: None,
			runtime: None,
		}
	}

	#[test]
	fn resolves_a_required_dependency_chain() {
		let candidates = vec![
			candidate(
				"root",
				"1.0.0",
				vec![require("lib", Predicate::new(Scheme::Semver, vec![">=1.0.0".to_string()]))],
			),
			candidate("lib", "1.0.0", Vec::new()),
			candidate("lib", "2.0.0", Vec::new()),
		];
		let lock = resolve(&request(), &context(), &candidates).expect("resolves");
		assert_eq!(lock.releases.len(), 2);
		let lib = lock.releases.iter().find(|release| release.project_id == "lib").expect("lib");
		assert_eq!(lib.human_version, "2.0.0");
	}

	#[test]
	fn reports_a_conflict_when_no_version_satisfies() {
		let candidates = vec![
			candidate(
				"root",
				"1.0.0",
				vec![require("lib", Predicate::new(Scheme::Exact, vec!["9.9.9".to_string()]))],
			),
			candidate("lib", "1.0.0", Vec::new()),
		];
		let error = resolve(&request(), &context(), &candidates).expect_err("conflict");
		assert!(matches!(error, ResolveError::NoCandidate { .. }));
	}

	#[test]
	fn detects_a_cycle() {
		let candidates = vec![
			candidate("root", "1.0.0", vec![require("a", Predicate::new(Scheme::Any, Vec::new()))]),
			candidate("a", "1.0.0", vec![require("root", Predicate::new(Scheme::Any, Vec::new()))]),
		];
		let error = resolve(&request(), &context(), &candidates).expect_err("cycle");
		assert!(matches!(error, ResolveError::Cycle { .. }));
	}

	#[test]
	fn skips_optional_dependencies() {
		let mut dependency = require("optional-lib", Predicate::new(Scheme::Any, Vec::new()));
		dependency.kind = DependencyKind::Optional;
		let candidates = vec![
			candidate("root", "1.0.0", vec![dependency]),
			candidate("optional-lib", "1.0.0", Vec::new()),
		];
		let lock = resolve(&request(), &context(), &candidates).expect("resolves");
		assert_eq!(lock.releases.len(), 1);
	}
}
