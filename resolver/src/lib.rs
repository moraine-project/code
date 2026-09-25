use std::collections::{BTreeMap, BTreeSet};

use moraine_model::compatibility::{Predicate, PredicateResult, Scheme, Side};
use moraine_model::dependency::{Dependency, DependencyKind, TargetKind};
use moraine_model::release::ReleasePayload;
use moraine_model::version::VersionCatalog;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Request {
	pub game_id: String,
	pub game_version: String,
	pub channel: Option<String>,
	pub loader_id: Option<String>,
	pub loader_version: Option<String>,
	pub runtime_id: Option<String>,
	pub runtime_version: Option<String>,
	pub side: Side,
	pub os: Option<String>,
	pub arch: Option<String>,
	pub root_project: String,
	pub root_predicate: Predicate,
}

#[derive(Debug, Clone)]
pub struct LoaderSupport {
	pub version: String,
	pub game_version_predicate: Predicate,
}

#[derive(Debug, Clone)]
pub struct Context {
	pub game: VersionCatalog,
	pub loader: Option<VersionCatalog>,
	pub runtime: Option<VersionCatalog>,
	pub loader_support: Vec<LoaderSupport>,
	pub loader_game_versions: Option<Predicate>,
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
	pub game_id: String,
	pub predicate_scheme: String,
	pub predicate_values: Vec<String>,
	pub applies_to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "LockfileWire")]
pub struct Lockfile {
	pub lockfile_version: u32,
	pub trust_policy_version: u32,
	pub game_id: String,
	pub game_version: String,
	#[serde(default)]
	pub channel: Option<String>,
	pub loader_id: Option<String>,
	pub loader_version: Option<String>,
	pub runtime_id: Option<String>,
	pub runtime_version: Option<String>,
	pub side: String,
	#[serde(default)]
	pub os: Option<String>,
	#[serde(default)]
	pub arch: Option<String>,
	pub releases: Vec<LockedRelease>,
	#[serde(default)]
	pub feeds: Vec<LockedFeed>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LockfileWire {
	lockfile_version: u32,
	trust_policy_version: u32,
	game_id: String,
	game_version: String,
	#[serde(default)]
	channel: Option<String>,
	loader_id: Option<String>,
	loader_version: Option<String>,
	runtime_id: Option<String>,
	runtime_version: Option<String>,
	side: String,
	#[serde(default)]
	os: Option<String>,
	#[serde(default)]
	arch: Option<String>,
	releases: Vec<LockedRelease>,
	#[serde(default)]
	feeds: Vec<LockedFeed>,
}

impl TryFrom<LockfileWire> for Lockfile {
	type Error = String;

	fn try_from(value: LockfileWire) -> Result<Self, Self::Error> {
		let lockfile = Self {
			lockfile_version: value.lockfile_version,
			trust_policy_version: value.trust_policy_version,
			game_id: value.game_id,
			game_version: value.game_version,
			channel: value.channel,
			loader_id: value.loader_id,
			loader_version: value.loader_version,
			runtime_id: value.runtime_id,
			runtime_version: value.runtime_version,
			side: value.side,
			os: value.os,
			arch: value.arch,
			releases: value.releases,
			feeds: value.feeds,
		};
		lockfile.validate()?;
		Ok(lockfile)
	}
}

impl Lockfile {
	pub fn validate(&self) -> Result<(), String> {
		if self.lockfile_version != 1 || self.trust_policy_version != 1 {
			return Err("unsupported lockfile or trust policy version".to_string());
		}
		if self.game_id.is_empty() || self.game_version.is_empty() || Side::parse(&self.side).is_none() {
			return Err("lockfile request context is incomplete".to_string());
		}
		let projects = self
			.releases
			.iter()
			.map(|release| release.project_id.as_str())
			.collect::<BTreeSet<_>>();
		if projects.len() != self.releases.len() || projects.contains("") {
			return Err("lockfile contains a duplicate or empty project".to_string());
		}
		for release in &self.releases {
			if release.project_id.is_empty() || release.release_id.is_empty() || release.human_version.is_empty() {
				return Err("lockfile contains an incomplete release".to_string());
			}
			if release.artifact.filename.is_empty() {
				return Err("lockfile contains an empty artifact filename".to_string());
			}
			let digest = release
				.artifact
				.digest
				.strip_prefix("sha256:")
				.ok_or_else(|| "lockfile artifact digest must use sha256".to_string())?;
			if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
				return Err("lockfile artifact digest is malformed".to_string());
			}
			for dependency in &release.dependencies {
				if !matches!(dependency.target_kind.as_str(), "project" | "loader" | "runtime")
					|| dependency.target_id.is_empty()
					|| dependency.game_id.is_empty()
					|| !matches!(dependency.kind.as_str(), "required")
					|| !matches!(dependency.applies_to.as_str(), "both" | "client" | "server")
					|| Scheme::parse(&dependency.predicate_scheme).is_none()
				{
					return Err("lockfile contains an invalid dependency".to_string());
				}
				if dependency.game_id != self.game_id {
					return Err("lockfile dependency names a different game".to_string());
				}
				if dependency.target_kind == "project" && !projects.contains(dependency.target_id.as_str()) {
					return Err("lockfile dependency names a missing project".to_string());
				}
			}
		}
		let mut remaining = projects.clone();
		while !remaining.is_empty() {
			let ready = self
				.releases
				.iter()
				.find(|release| {
					remaining.contains(release.project_id.as_str())
						&& release.dependencies.iter().all(|dependency| {
							dependency.target_kind != "project" || !remaining.contains(dependency.target_id.as_str())
						})
				})
				.map(|release| release.project_id.as_str());
			let Some(ready) = ready else {
				return Err("lockfile dependency graph contains a cycle".to_string());
			};
			remaining.remove(ready);
		}
		Ok(())
	}
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
	CrossGame {
		project_id: String,
		game_id: String,
	},
	MissingArtifact {
		project_id: String,
	},
	LoaderIncompatible {
		loader_id: String,
		loader_version: String,
		game_version: String,
	},
	TooDeep,
	InvalidLockfile(String),
	Cycle {
		project_id: String,
	},
}

impl std::fmt::Display for ResolveError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::NoCandidate { project_id, detail } => write!(f, "no candidate for `{project_id}`: {detail}"),
			Self::Conflict { project_id, detail } => write!(f, "conflict on `{project_id}`: {detail}"),
			Self::CrossGame { project_id, game_id } => {
				write!(f, "`{project_id}` targets game `{game_id}`, not this one")
			}
			Self::MissingArtifact { project_id } => write!(f, "`{project_id}` has no primary artifact"),
			Self::LoaderIncompatible {
				loader_id,
				loader_version,
				game_version,
			} => write!(
				f,
				"loader `{loader_id}` version `{loader_version}` does not support game version `{game_version}`"
			),
			Self::TooDeep => f.write_str("dependency graph is too deep to resolve"),
			Self::InvalidLockfile(detail) => write!(f, "invalid lockfile: {detail}"),
			Self::Cycle { project_id } => write!(f, "dependency cycle reaches `{project_id}`"),
		}
	}
}

impl std::error::Error for ResolveError {}

const MAX_DEPTH: usize = 64;

pub fn resolve<'a>(request: &Request, context: &Context, candidates: &'a [Candidate]) -> Result<Lockfile, ResolveError> {
	check_loader_support(request, context)?;
	let mut by_project: BTreeMap<&'a str, Vec<&'a Candidate>> = BTreeMap::new();
	for candidate in candidates {
		by_project.entry(candidate.project_id.as_str()).or_default().push(candidate);
	}
	for group in by_project.values_mut() {
		group.sort_by(|a, b| compare_versions(&context.game, &b.human_version, &a.human_version));
	}

	let mut selected: BTreeMap<String, &'a Candidate> = BTreeMap::new();
	solve(
		request,
		context,
		&by_project,
		vec![(request.root_project.clone(), request.root_predicate.clone())],
		&mut selected,
	)?;

	let mut releases = Vec::new();
	for (project_id, candidate) in &selected {
		let artifact = primary_artifact(candidate, request).ok_or_else(|| ResolveError::MissingArtifact {
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
				.filter(|dependency| {
					applies_to_side(dependency, request.side) && matches!(dependency.kind, DependencyKind::Required)
				})
				.map(|dependency| LockedDependency {
					target_kind: dependency.target_kind.as_str().to_string(),
					target_id: dependency.target_id.clone(),
					kind: dependency.kind.as_str().to_string(),
					game_id: dependency.game_id.clone(),
					predicate_scheme: dependency.predicate.scheme.clone(),
					predicate_values: dependency.predicate.values.clone(),
					applies_to: dependency.applies_to.as_str().to_string(),
				})
				.collect(),
		});
	}
	releases.sort_by(|a, b| a.project_id.cmp(&b.project_id));
	sort_install_order(&mut releases);

	let lockfile = Lockfile {
		lockfile_version: 1,
		trust_policy_version: 1,
		game_id: request.game_id.clone(),
		game_version: request.game_version.clone(),
		channel: request.channel.clone(),
		loader_id: request.loader_id.clone(),
		loader_version: request.loader_version.clone(),
		runtime_id: request.runtime_id.clone(),
		runtime_version: request.runtime_version.clone(),
		side: request.side.as_str().to_string(),
		os: request.os.clone(),
		arch: request.arch.clone(),
		releases,
		feeds: Vec::new(),
	};
	lockfile.validate().map_err(ResolveError::InvalidLockfile)?;
	Ok(lockfile)
}

fn check_loader_support(request: &Request, context: &Context) -> Result<(), ResolveError> {
	let Some(loader_id) = request.loader_id.as_deref() else {
		return Ok(());
	};
	if let Some(loader_version) = request.loader_version.as_deref() {
		let for_version: Vec<&LoaderSupport> = context
			.loader_support
			.iter()
			.filter(|support| support.version == loader_version)
			.collect();
		if !for_version.is_empty() {
			let supported = for_version.iter().any(|support| {
				satisfies_catalog(&support.game_version_predicate, &request.game_version, Some(&context.game))
			});
			if supported {
				return Ok(());
			}
			return Err(ResolveError::LoaderIncompatible {
				loader_id: loader_id.to_string(),
				loader_version: loader_version.to_string(),
				game_version: request.game_version.clone(),
			});
		}
	}
	if let Some(family) = &context.loader_game_versions
		&& !satisfies_catalog(family, &request.game_version, Some(&context.game))
	{
		return Err(ResolveError::LoaderIncompatible {
			loader_id: loader_id.to_string(),
			loader_version: request.loader_version.clone().unwrap_or_default(),
			game_version: request.game_version.clone(),
		});
	}
	Ok(())
}

fn solve<'a>(
	request: &Request,
	context: &Context,
	by_project: &BTreeMap<&'a str, Vec<&'a Candidate>>,
	mut queue: Vec<(String, Predicate)>,
	selected: &mut BTreeMap<String, &'a Candidate>,
) -> Result<(), ResolveError> {
	let mut trail = Vec::new();
	while let Some((project_id, predicate)) = queue.first().cloned() {
		queue.remove(0);
		let checkpoint = trail.len();
		if let Err(error) = ensure_project(
			request,
			context,
			by_project,
			&project_id,
			&predicate,
			BTreeSet::new(),
			selected,
			&mut trail,
		) {
			rollback(&mut trail, checkpoint, selected);
			return Err(error);
		}
	}
	Ok(())
}

#[allow(clippy::too_many_arguments)]
fn ensure_project<'a>(
	request: &Request,
	context: &Context,
	by_project: &BTreeMap<&'a str, Vec<&'a Candidate>>,
	project_id: &str,
	predicate: &Predicate,
	ancestors: BTreeSet<String>,
	selected: &mut BTreeMap<String, &'a Candidate>,
	trail: &mut Vec<String>,
) -> Result<(), ResolveError> {
	if ancestors.len() > MAX_DEPTH || selected.len() > MAX_DEPTH {
		return Err(ResolveError::TooDeep);
	}
	if ancestors.contains(project_id) {
		return Err(ResolveError::Cycle {
			project_id: project_id.to_string(),
		});
	}
	if let Some(existing) = selected.get(project_id) {
		if satisfies_catalog(predicate, &existing.human_version, Some(&context.game)) {
			return Ok(());
		}
		return Err(ResolveError::Conflict {
			project_id: project_id.to_string(),
			detail: format!(
				"selected {} does not satisfy an additional constraint",
				existing.human_version
			),
		});
	}

	let candidates = by_project.get(project_id).cloned().unwrap_or_default();
	let mut last_error = None;
	for candidate in candidates {
		if !compatible(request, context, candidate) {
			continue;
		}
		if !satisfies_catalog(predicate, &candidate.human_version, Some(&context.game)) {
			continue;
		}
		let mut dependencies = Vec::new();
		if let Err(error) = collect_dependencies(request, context, candidate, &mut dependencies) {
			last_error = Some(error);
			continue;
		}
		let checkpoint = trail.len();
		let inserted = !selected.contains_key(project_id);
		if inserted {
			trail.push(project_id.to_string());
		}
		selected.insert(project_id.to_string(), candidate);
		let mut next_ancestors = ancestors.clone();
		next_ancestors.insert(project_id.to_string());
		let result = dependencies.iter().try_for_each(|(target, constraint)| {
			ensure_project(
				request,
				context,
				by_project,
				target,
				constraint,
				next_ancestors.clone(),
				selected,
				trail,
			)
		});
		if result.is_ok() {
			return Ok(());
		}
		let error = result.unwrap_err();
		rollback(trail, checkpoint, selected);
		if inserted {
			selected.remove(project_id);
		}
		last_error = Some(error);
	}
	if let Some(error) = last_error {
		Err(error)
	} else {
		Err(ResolveError::NoCandidate {
			project_id: project_id.to_string(),
			detail: "no release matches the request, the predicate, and its dependencies".to_string(),
		})
	}
}

fn rollback(trail: &mut Vec<String>, checkpoint: usize, selected: &mut BTreeMap<String, &Candidate>) {
	while trail.len() > checkpoint {
		if let Some(project_id) = trail.pop() {
			selected.remove(&project_id);
		}
	}
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
		if dependency.game_id != request.game_id {
			return Err(ResolveError::CrossGame {
				project_id: dependency.target_id.clone(),
				game_id: dependency.game_id.clone(),
			});
		}
		match dependency.target_kind {
			TargetKind::Project => {
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
		&& request
			.channel
			.as_ref()
			.is_none_or(|channel| &candidate.payload.channel == channel)
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
			let platform_ok = platform_matches(entry.os_predicate.as_deref(), request.os.as_deref())
				&& platform_matches(entry.arch_predicate.as_deref(), request.arch.as_deref());
			game_ok && side_ok && loader_ok && runtime_ok && platform_ok
		}) && entry_side_supported(candidate, request.side)
		&& primary_artifact(candidate, request).is_some()
}

fn platform_matches(predicate: Option<&[String]>, wanted: Option<&str>) -> bool {
	let Some(predicate) = predicate else {
		return true;
	};
	predicate.iter().any(|value| value == "any" || wanted == Some(value.as_str()))
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

fn satisfies_catalog(predicate: &Predicate, version: &str, catalog: Option<&VersionCatalog>) -> bool {
	match predicate.scheme() {
		Some(Scheme::Any) => true,
		Some(Scheme::Exact) | Some(Scheme::Set) => predicate.values.iter().any(|candidate| candidate == version),
		Some(Scheme::Semver) => {
			catalog.is_some_and(|catalog| catalog.evaluate(predicate, version) == PredicateResult::Satisfied)
		}
		Some(Scheme::OrderedList) | Some(Scheme::Calendar) => {
			catalog.is_some_and(|catalog| catalog.evaluate(predicate, version) == PredicateResult::Satisfied)
		}
		None => false,
	}
}

fn sort_install_order(releases: &mut Vec<LockedRelease>) {
	let mut ordered: Vec<LockedRelease> = Vec::with_capacity(releases.len());
	while !releases.is_empty() {
		let position = releases.iter().position(|release| {
			release.dependencies.iter().all(|dependency| {
				dependency.target_kind != "project"
					|| ordered.iter().any(|installed| installed.project_id == dependency.target_id)
			})
		});
		let position = position.unwrap_or(0);
		ordered.push(releases.remove(position));
	}
	*releases = ordered;
}

fn primary_artifact<'a>(candidate: &'a Candidate, request: &Request) -> Option<&'a moraine_model::artifact::Artifact> {
	let mut matches = candidate.payload.artifacts.iter().filter(|artifact| {
		artifact.is_primary
			&& platform_matches(artifact.os_predicate.as_deref(), request.os.as_deref())
			&& platform_matches(artifact.arch_predicate.as_deref(), request.arch.as_deref())
	});
	let artifact = matches.next()?;
	if matches.next().is_some() {
		return None;
	}
	Some(artifact)
}

fn compare_versions(catalog: &VersionCatalog, a: &str, b: &str) -> std::cmp::Ordering {
	catalog.compare(a, b).unwrap_or(std::cmp::Ordering::Equal)
}

#[cfg(test)]
mod tests {
	use moraine_model::artifact::Artifact;
	use moraine_model::compatibility::Compatibility;
	use moraine_model::dependency::{Dependency, DependencyKind, TargetKind};
	use moraine_model::release::ReleasePayload;
	use moraine_model::version::OrderingScheme;

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
			channel: None,
			loader_id: None,
			loader_version: None,
			runtime_id: None,
			runtime_version: None,
			side: Side::Client,
			os: None,
			arch: None,
			root_project: "root".to_string(),
			root_predicate: Predicate::new(Scheme::Any, Vec::new()),
		}
	}

	fn context() -> Context {
		Context {
			game: VersionCatalog::new(OrderingScheme::Semver, Vec::new()),
			loader: None,
			runtime: None,
			loader_support: Vec::new(),
			loader_game_versions: None,
		}
	}

	fn loader_context(game_version_predicate: Predicate) -> Context {
		Context {
			game: VersionCatalog::new(OrderingScheme::Semver, Vec::new()),
			loader: None,
			runtime: None,
			loader_support: vec![LoaderSupport {
				version: "0.15.0".to_string(),
				game_version_predicate,
			}],
			loader_game_versions: None,
		}
	}

	#[test]
	fn refuses_a_loader_version_that_does_not_support_the_game_version() {
		let mut request = request();
		request.loader_id = Some("gd:sha256:loader".to_string());
		request.loader_version = Some("0.15.0".to_string());
		let context = loader_context(Predicate::new(Scheme::Exact, vec!["1.19.0".to_string()]));

		let error = resolve(&request, &context, &[]).expect_err("incompatible loader");
		assert!(matches!(error, ResolveError::LoaderIncompatible { .. }), "{error}");
	}

	#[test]
	fn accepts_a_loader_version_that_supports_the_game_version() {
		let mut request = request();
		request.loader_id = Some("gd:sha256:loader".to_string());
		request.loader_version = Some("0.15.0".to_string());
		let context = loader_context(Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]));

		let error = resolve(&request, &context, &[]).expect_err("no candidates");
		assert!(matches!(error, ResolveError::NoCandidate { .. }), "{error}");
	}

	#[test]
	fn an_unknown_loader_version_is_not_treated_as_incompatible() {
		let mut request = request();
		request.loader_id = Some("gd:sha256:loader".to_string());
		request.loader_version = Some("9.9.9".to_string());
		let context = loader_context(Predicate::new(Scheme::Exact, vec!["1.19.0".to_string()]));

		let error = resolve(&request, &context, &[]).expect_err("no candidates");
		assert!(matches!(error, ResolveError::NoCandidate { .. }), "{error}");
	}

	#[test]
	fn refuses_a_loader_family_that_does_not_support_the_game_version() {
		let mut request = request();
		request.loader_id = Some("gd:sha256:loader".to_string());
		let mut context = context();
		context.loader_game_versions = Some(Predicate::new(Scheme::Exact, vec!["1.19.0".to_string()]));

		let error = resolve(&request, &context, &[]).expect_err("incompatible loader family");
		assert!(matches!(error, ResolveError::LoaderIncompatible { .. }), "{error}");
	}

	#[test]
	fn accepts_a_loader_family_that_supports_the_game_version() {
		let mut request = request();
		request.loader_id = Some("gd:sha256:loader".to_string());
		let mut context = context();
		context.loader_game_versions = Some(Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]));

		let error = resolve(&request, &context, &[]).expect_err("no candidates");
		assert!(matches!(error, ResolveError::NoCandidate { .. }), "{error}");
	}

	#[test]
	fn a_known_loader_version_outranks_the_family_declaration() {
		let mut request = request();
		request.loader_id = Some("gd:sha256:loader".to_string());
		request.loader_version = Some("0.15.0".to_string());
		let mut context = loader_context(Predicate::new(Scheme::Exact, vec!["1.19.0".to_string()]));
		context.loader_game_versions = Some(Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]));

		let error = resolve(&request, &context, &[]).expect_err("the release does not support 1.20.1");
		assert!(matches!(error, ResolveError::LoaderIncompatible { .. }), "{error}");
	}

	#[test]
	fn filters_by_requested_channel() {
		let mut release = candidate("root", "1.0.0", Vec::new());
		release.payload.channel = "beta".to_string();
		let mut request = request();
		request.channel = Some("release".to_string());
		assert!(resolve(&request, &context(), &[release]).is_err());
	}

	#[test]
	fn selects_a_primary_artifact_for_the_requested_platform() {
		let mut release = candidate("root", "1.0.0", Vec::new());
		release.payload.artifacts = vec![
			Artifact {
				digest: vec![0x11; 32],
				size: 10,
				media_type: "application/java-archive".to_string(),
				filename: "windows.jar".to_string(),
				is_primary: true,
				os_predicate: Some(vec!["windows".to_string()]),
				arch_predicate: None,
			},
			Artifact {
				digest: vec![0x22; 32],
				size: 10,
				media_type: "application/java-archive".to_string(),
				filename: "linux.jar".to_string(),
				is_primary: true,
				os_predicate: Some(vec!["linux".to_string()]),
				arch_predicate: None,
			},
		];
		let mut request = request();
		request.os = Some("linux".to_string());
		let lock = resolve(&request, &context(), &[release]).expect("resolves");
		assert_eq!(lock.releases[0].artifact.digest, format!("sha256:{}", "22".repeat(32)));
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
		assert_eq!(lock.releases[0].project_id, "lib");
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
	fn refuses_a_mutual_dependency_cycle() {
		let candidates = vec![
			candidate("root", "1.0.0", vec![require("a", Predicate::new(Scheme::Any, Vec::new()))]),
			candidate("a", "1.0.0", vec![require("root", Predicate::new(Scheme::Any, Vec::new()))]),
		];
		let error = resolve(&request(), &context(), &candidates).expect_err("a dependency cycle is invalid");
		assert!(matches!(error, ResolveError::Cycle { .. }), "{error}");
	}

	#[test]
	fn refuses_a_release_that_excludes_the_requested_platform() {
		let mut windows = candidate("root", "1.0.0", Vec::new());
		windows.payload.compatibility[0].os_predicate = Some(vec!["windows".to_string()]);
		let mut linux = candidate("root", "2.0.0", Vec::new());
		linux.payload.compatibility[0].os_predicate = Some(vec!["linux".to_string()]);

		let mut windows_only = candidate("root", "1.0.0", Vec::new());
		windows_only.payload.compatibility[0].os_predicate = Some(vec!["windows".to_string()]);
		let mut request = request();
		request.os = Some("linux".to_string());
		let lock = resolve(&request, &context(), &[windows, linux]).expect("the linux build is chosen");
		assert_eq!(lock.releases[0].human_version, "2.0.0");

		request.os = Some("macos".to_string());
		assert!(
			resolve(&request, &context(), &[windows_only]).is_err(),
			"a platform no build names cannot be satisfied"
		);
	}

	#[test]
	fn a_release_without_a_platform_predicate_matches_any_platform() {
		let candidates = vec![candidate("root", "1.0.0", Vec::new())];
		let mut request = request();
		request.os = Some("linux".to_string());
		assert!(
			resolve(&request, &context(), &candidates).is_ok(),
			"a release that names no platform is not platform-specific"
		);
	}

	#[test]
	fn refuses_a_platform_specific_compatibility_when_the_request_omits_the_platform() {
		let mut release = candidate("root", "1.0.0", Vec::new());
		release.payload.compatibility[0].os_predicate = Some(vec!["windows".to_string()]);
		assert!(resolve(&request(), &context(), &[release]).is_err());
	}

	#[test]
	fn refuses_a_platform_specific_artifact_when_the_request_omits_the_platform() {
		let mut release = candidate("root", "1.0.0", Vec::new());
		release.payload.artifacts[0].os_predicate = Some(vec!["windows".to_string()]);
		assert!(resolve(&request(), &context(), &[release]).is_err());
	}

	#[test]
	fn orders_candidates_with_the_games_declared_scheme() {
		let mut context = context();
		context.game = VersionCatalog::new(
			OrderingScheme::OrderedList,
			vec!["1.0".to_string(), "1.2".to_string(), "1.10".to_string()],
		);
		let mut candidates = vec![
			candidate("root", "1.0", Vec::new()),
			candidate("root", "1.10", Vec::new()),
			candidate("root", "1.2", Vec::new()),
		];
		for candidate in &mut candidates {
			candidate.payload.compatibility[0].game_version_predicate = Predicate::new(Scheme::Any, Vec::new());
		}
		let mut request = request();
		request.game_version = "1.10".to_string();
		let lock = resolve(&request, &context, &candidates).expect("resolves");
		assert_eq!(
			lock.releases[0].human_version, "1.10",
			"an ordered-list game must not be sorted as semver"
		);
	}

	#[test]
	fn resolves_a_diamond_dependency() {
		let any = || Predicate::new(Scheme::Any, Vec::new());
		let candidates = vec![
			candidate("root", "1.0.0", vec![require("left", any()), require("right", any())]),
			candidate("left", "1.0.0", vec![require("shared", any())]),
			candidate("right", "1.0.0", vec![require("shared", any())]),
			candidate("shared", "1.0.0", Vec::new()),
		];
		let lock = resolve(&request(), &context(), &candidates).expect("a diamond is not a cycle");
		let mut ids: Vec<&str> = lock.releases.iter().map(|release| release.project_id.as_str()).collect();
		ids.sort_unstable();
		assert_eq!(ids, vec!["left", "right", "root", "shared"]);
	}

	#[test]
	fn deserializing_a_lockfile_runs_validation() {
		let value = serde_json::json!({
			"lockfile_version": 1,
			"trust_policy_version": 1,
			"game_id": "gd:sha256:game",
			"game_version": "1.0.0",
			"loader_id": null,
			"loader_version": null,
			"runtime_id": null,
			"runtime_version": null,
			"side": "sideways",
			"releases": []
		});
		assert!(serde_json::from_value::<Lockfile>(value).is_err());
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
