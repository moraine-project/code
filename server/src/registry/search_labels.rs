use moraine_model::compatibility::Scheme;
use moraine_model::release::ReleasePayload;

use crate::db::MetadataStore;
use crate::routes::AppState;

pub(crate) const LOADER_VERSION_SEPARATOR: char = '\u{1f}';

pub(crate) fn loader_version_label(loader_id: &str, version: &str) -> String {
	format!("{loader_id}{LOADER_VERSION_SEPARATOR}{version}")
}

impl MetadataStore {
	pub async fn add_search_labels(&self, project_id: &str, label_kind: &str, labels: &[String]) -> Result<(), sqlx::Error> {
		for label in labels {
			sqlx::query(
				"INSERT INTO search_labels (project_id, label_kind, label_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
			)
			.bind(project_id)
			.bind(label_kind)
			.bind(label)
			.execute(&self.pool)
			.await?;
		}
		Ok(())
	}
}

pub(crate) async fn index_release_labels(state: &AppState, release: &ReleasePayload) -> Result<(), sqlx::Error> {
	let project_id = &release.project_id;
	let loaders: Vec<String> = release
		.compatibility
		.iter()
		.filter_map(|entry| entry.loader_id.clone())
		.collect();
	if !loaders.is_empty() {
		state.metadata.add_search_labels(project_id, "loader", &loaders).await?;
	}
	let versions: Vec<String> = release
		.compatibility
		.iter()
		.filter(|entry| matches!(entry.game_version_predicate.scheme(), Some(Scheme::Exact) | Some(Scheme::Set)))
		.flat_map(|entry| entry.game_version_predicate.values.iter().cloned())
		.collect();
	if !versions.is_empty() {
		state
			.metadata
			.add_search_labels(project_id, "game-version", &versions)
			.await?;
	}
	state
		.metadata
		.add_search_labels(project_id, "channel", std::slice::from_ref(&release.channel))
		.await?;
	let platforms: Vec<String> = release
		.artifacts
		.iter()
		.filter_map(|artifact| artifact.os_predicate.as_ref())
		.chain(release.compatibility.iter().filter_map(|entry| entry.os_predicate.as_ref()))
		.flatten()
		.cloned()
		.collect();
	if !platforms.is_empty() {
		state.metadata.add_search_labels(project_id, "platform", &platforms).await?;
	}
	let mut loader_versions = Vec::new();
	let mut runtime_versions = Vec::new();
	for entry in &release.compatibility {
		if let (Some(loader), Some(predicate)) = (&entry.loader_id, &entry.loader_version_predicate)
			&& matches!(predicate.scheme(), Some(Scheme::Exact) | Some(Scheme::Set))
		{
			for version in &predicate.values {
				loader_versions.push(loader_version_label(loader, version));
			}
		}
		if let Some(predicate) = &entry.runtime_predicate
			&& matches!(predicate.scheme(), Some(Scheme::Exact) | Some(Scheme::Set))
		{
			runtime_versions.extend(predicate.values.iter().cloned());
		}
	}
	if !loader_versions.is_empty() {
		state
			.metadata
			.add_search_labels(project_id, "loader-version", &loader_versions)
			.await?;
	}
	if !runtime_versions.is_empty() {
		state
			.metadata
			.add_search_labels(project_id, "runtime-version", &runtime_versions)
			.await?;
	}
	Ok(())
}
