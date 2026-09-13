use moraine_crypto::SigningKey;
use serde::Serialize;

use crate::config::Config;

#[derive(Debug, Clone, Serialize)]
pub struct Capability {
	pub protocol_versions: Vec<u32>,
	pub max_artifact_bytes: u64,
	pub max_upload_bytes_per_account: u64,
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
	pub registration: String,
	pub webhook_public_key: Option<String>,
	#[serde(skip)]
	pub max_projects: u64,
	#[serde(skip)]
	pub max_definitions: u64,
	#[serde(skip)]
	pub max_sync_entries: u32,
	#[serde(skip)]
	pub max_mirror_probes_per_cycle: u32,
	#[serde(skip)]
	pub max_mirror_probe_bytes: u64,
	#[serde(skip)]
	pub allow_insecure_federation_local: bool,
	#[serde(skip)]
	pub web_origins: Vec<String>,
	#[serde(skip)]
	pub registration_open: bool,
	#[serde(skip)]
	pub tls_extra_roots: Vec<reqwest::Certificate>,
	#[serde(skip)]
	pub webhook_signer: Option<SigningKey>,
}

impl Capability {
	pub fn is_open(&self) -> bool {
		self.publishing == "open"
	}

	pub fn cross_origin(&self) -> bool {
		!self.web_origins.is_empty()
	}

	pub fn origin_allowed(&self, origin: &str) -> bool {
		let origin = origin.trim_end_matches('/');
		self.web_origins.iter().any(|allowed| allowed.eq_ignore_ascii_case(origin))
	}

	pub fn discover(config: &Config) -> Self {
		let webhook_signer = load_or_create_webhook_key(config);
		let webhook_public_key = webhook_signer.as_ref().map(|key| hex::encode(key.verifying_key().to_bytes()));
		Self {
			protocol_versions: vec![1],
			max_artifact_bytes: config.max_artifact_bytes,
			max_upload_bytes_per_account: config.max_upload_bytes_per_account,
			max_feed_page_entries: config.max_feed_page_entries,
			max_feed_scan_pages: config.max_feed_scan_pages,
			max_response_bytes: config.max_response_bytes,
			max_sync_pages: config.max_sync_pages,
			requests_per_minute: config.requests_per_minute,
			max_concurrent_syncs: config.max_concurrent_syncs,
			max_projects: config.max_projects,
			max_definitions: config.max_definitions,
			max_sync_entries: config.max_sync_entries,
			max_mirror_probes_per_cycle: config.max_mirror_probes_per_cycle,
			max_mirror_probe_bytes: config.max_mirror_probe_bytes,
			maintenance_interval_seconds: config.maintenance_interval_seconds,
			artifact_sources: vec!["local".to_string(), "external".to_string(), "mirrored".to_string()],
			upload_modes: vec!["staged".to_string()],
			server_role: vec!["home".to_string(), "directory".to_string()],
			publishing: config.publishing.as_str().to_string(),
			registration: config.registration.as_str().to_string(),
			webhook_public_key,
			allow_insecure_federation_local: config.allow_insecure_federation_local,
			registration_open: config.registration.is_open(),
			web_origins: config
				.web_origins
				.iter()
				.map(|origin| origin.trim().trim_end_matches('/').to_string())
				.filter(|origin| !origin.is_empty())
				.collect(),
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
	if path.exists() {
		return match read_key_file(&path) {
			Some(key) => Some(key),
			None => {
				tracing::error!(
					path = %path.display(),
					"the webhook key file is malformed; webhooks stay disabled rather than rotating the key"
				);
				None
			}
		};
	}
	let mut seed = [0u8; 32];
	if getrandom::fill(&mut seed).is_err() {
		return None;
	}
	if std::fs::create_dir_all(&config.data_dir).is_err() {
		return None;
	}
	let mut options = std::fs::OpenOptions::new();
	options.write(true).create_new(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		options.mode(0o600);
	}
	match options.open(&path) {
		Ok(mut file) => {
			use std::io::Write;
			if file.write_all(format!("{}\n", hex::encode(seed)).as_bytes()).is_err() {
				return None;
			}
			Some(SigningKey::from_seed(&seed))
		}
		Err(_) => read_key_file(&path),
	}
}

fn read_key_file(path: &std::path::Path) -> Option<SigningKey> {
	let text = std::fs::read_to_string(path).ok()?;
	let bytes = hex::decode(text.trim()).ok()?;
	let seed = <[u8; 32]>::try_from(bytes.as_slice()).ok()?;
	Some(SigningKey::from_seed(&seed))
}
