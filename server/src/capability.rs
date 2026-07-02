use serde::Serialize;

use crate::config::Config;

#[derive(Debug, Clone, Serialize)]
pub struct Capability {
	pub protocol_versions: Vec<u32>,
	pub max_artifact_bytes: u64,
	pub max_feed_page_entries: u32,
	pub artifact_sources: Vec<String>,
	pub upload_modes: Vec<String>,
	pub server_role: Vec<String>,
	pub publishing: String,
}

impl Capability {
	pub fn is_open(&self) -> bool {
		self.publishing == "open"
	}

	pub fn discover(config: &Config) -> Self {
		Self {
			protocol_versions: vec![1],
			max_artifact_bytes: config.max_artifact_bytes,
			max_feed_page_entries: config.max_feed_page_entries,
			artifact_sources: vec!["local".to_string(), "external".to_string(), "mirrored".to_string()],
			upload_modes: vec!["staged".to_string()],
			server_role: vec!["home".to_string(), "directory".to_string()],
			publishing: config.publishing.as_str().to_string(),
		}
	}
}
