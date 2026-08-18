use moraine_crypto::SigningKey;
use serde::Serialize;

use crate::config::Config;

#[derive(Debug, Clone, Serialize)]
pub struct Capability {
	pub protocol_versions: Vec<u32>,
	pub max_artifact_bytes: u64,
	pub max_feed_page_entries: u32,
	pub max_feed_scan_pages: u32,
	pub max_response_bytes: u64,
	pub max_sync_pages: u32,
	pub requests_per_minute: u32,
	pub max_concurrent_syncs: u32,
	pub maintenance_interval_seconds: u64,
	pub artifact_sources: Vec<String>,
	pub upload_modes: Vec<String>,
	pub server_role: Vec<String>,
	pub publishing: String,
	pub webhook_public_key: Option<String>,
	#[serde(skip)]
	pub allow_insecure_federation_local: bool,
	#[serde(skip)]
	pub tls_extra_roots: Vec<reqwest::Certificate>,
	#[serde(skip)]
	pub webhook_signer: Option<SigningKey>,
}

impl Capability {
	pub fn is_open(&self) -> bool {
		self.publishing == "open"
	}

	pub fn discover(config: &Config) -> Self {
		let webhook_signer = load_or_create_webhook_key(config);
		let webhook_public_key = webhook_signer.as_ref().map(|key| hex::encode(key.verifying_key().to_bytes()));
		Self {
			protocol_versions: vec![1],
			max_artifact_bytes: config.max_artifact_bytes,
			max_feed_page_entries: config.max_feed_page_entries,
			max_feed_scan_pages: config.max_feed_scan_pages,
			max_response_bytes: config.max_response_bytes,
			max_sync_pages: config.max_sync_pages,
			requests_per_minute: config.requests_per_minute,
			max_concurrent_syncs: config.max_concurrent_syncs,
			maintenance_interval_seconds: config.maintenance_interval_seconds,
			artifact_sources: vec!["local".to_string(), "external".to_string(), "mirrored".to_string()],
			upload_modes: vec!["staged".to_string()],
			server_role: vec!["home".to_string(), "directory".to_string()],
			publishing: config.publishing.as_str().to_string(),
			webhook_public_key,
			allow_insecure_federation_local: config.allow_insecure_federation_local,
			tls_extra_roots: extra_roots(config),
			webhook_signer,
		}
	}
}

fn extra_roots(config: &Config) -> Vec<reqwest::Certificate> {
	let Some(path) = config.tls_extra_roots.as_deref() else {
		return Vec::new();
	};
	let (roots, skipped) = crate::federation::egress::load_extra_roots(path);
	if skipped > 0 {
		tracing::warn!(path = %path.display(), skipped, "some TLS roots were not parsed");
	}
	roots
}

fn load_or_create_webhook_key(config: &Config) -> Option<SigningKey> {
	let path = config.data_dir.join("webhook.key");
	if let Ok(text) = std::fs::read_to_string(&path)
		&& let Ok(bytes) = hex::decode(text.trim())
		&& let Ok(seed) = <[u8; 32]>::try_from(bytes.as_slice())
	{
		return Some(SigningKey::from_seed(&seed));
	}
	let mut seed = [0u8; 32];
	if getrandom::fill(&mut seed).is_err() {
		return None;
	}
	if std::fs::create_dir_all(&config.data_dir).is_err() {
		return None;
	}
	if std::fs::write(&path, format!("{}\n", hex::encode(seed))).is_err() {
		return None;
	}
	Some(SigningKey::from_seed(&seed))
}
