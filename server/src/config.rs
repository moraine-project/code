use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Args, Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Publishing {
	Review,
	Progressive,
	Open,
}

impl Publishing {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Review => "review",
			Self::Progressive => "progressive",
			Self::Open => "open",
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Registration {
	Open,
	Closed,
}

impl Registration {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Open => "open",
			Self::Closed => "closed",
		}
	}

	pub const fn is_open(self) -> bool {
		matches!(self, Self::Open)
	}
}

#[derive(Debug, Clone, Parser)]
pub struct Cli {
	#[command(flatten)]
	pub config: Config,

	#[command(subcommand)]
	pub command: Option<Command>,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum Command {
	Bootstrap {
		#[arg(long, env = "MORAINE_OPERATOR_EMAIL")]
		email: String,
	},
	Backup {
		#[arg(long)]
		out: PathBuf,
	},
	VerifyBackup {
		#[arg(long)]
		dir: PathBuf,
	},
	Restore {
		#[arg(long)]
		dir: PathBuf,
		#[arg(long)]
		force: bool,
	},
	Migrate,
}

#[derive(Debug, Clone, Parser)]
pub struct Config {
	#[arg(long, env = "MORAINE_BIND", default_value = "127.0.0.1:8080")]
	pub bind: SocketAddr,

	#[arg(long, env = "MORAINE_TLS_TERMINATED", default_value_t = false)]
	pub tls_terminated: bool,

	#[arg(long, env = "MORAINE_ALLOW_INSECURE_HTTP", default_value_t = false)]
	pub allow_insecure_http: bool,

	#[arg(long, env = "MORAINE_WEB_ORIGINS", value_delimiter = ',')]
	pub web_origins: Vec<String>,

	#[arg(long, env = "MORAINE_DATA_DIR", default_value = "./data")]
	pub data_dir: PathBuf,

	#[arg(long, env = "MORAINE_MAX_ARTIFACT_BYTES", default_value_t = 536_870_912)]
	pub max_artifact_bytes: u64,

	#[arg(long, env = "MORAINE_MAX_UPLOAD_BYTES_PER_ACCOUNT", default_value_t = 5_368_709_120)]
	pub max_upload_bytes_per_account: u64,

	#[arg(long, env = "MORAINE_MAX_PROJECTS", default_value_t = 10_000)]
	pub max_projects: u64,

	#[arg(long, env = "MORAINE_MAX_DEFINITIONS", default_value_t = 1_000)]
	pub max_definitions: u64,

	#[arg(long, env = "MORAINE_MAX_SYNC_ENTRIES", default_value_t = 10_000)]
	pub max_sync_entries: u32,

	#[arg(long, env = "MORAINE_METRICS_TOKEN")]
	pub metrics_token: Option<String>,

	#[arg(long, env = "MORAINE_SMTP_URL")]
	pub smtp_url: Option<String>,

	#[arg(long, env = "MORAINE_MAIL_FROM")]
	pub mail_from: Option<String>,

	#[arg(long, env = "MORAINE_PUBLIC_URL")]
	pub public_url: Option<String>,

	#[arg(long, env = "MORAINE_REQUIRE_VERIFIED_EMAIL", default_value_t = false)]
	pub require_verified_email: bool,

	#[arg(long, env = "MORAINE_MAX_MIRROR_PROBES_PER_CYCLE", default_value_t = 20)]
	pub max_mirror_probes_per_cycle: u32,

	#[arg(long, env = "MORAINE_MAX_MIRROR_PROBE_BYTES", default_value_t = 268_435_456)]
	pub max_mirror_probe_bytes: u64,

	#[arg(long, env = "MORAINE_MAX_FEED_PAGE_ENTRIES", default_value_t = 100)]
	pub max_feed_page_entries: u32,

	#[arg(long, env = "MORAINE_MAX_RESPONSE_BYTES", default_value_t = 16_777_216)]
	pub max_response_bytes: u64,

	#[arg(long, env = "MORAINE_STAGING_RETENTION_SECONDS", default_value_t = 3_600)]
	pub staging_retention_seconds: u64,

	#[arg(long, env = "MORAINE_BLOB_RETENTION_SECONDS", default_value_t = 604_800)]
	pub blob_retention_seconds: u64,

	#[arg(long, env = "MORAINE_MAX_SYNC_PAGES", default_value_t = 200)]
	pub max_sync_pages: u32,

	#[arg(long, env = "MORAINE_REQUESTS_PER_MINUTE", default_value_t = 600)]
	pub requests_per_minute: u32,

	#[arg(long, env = "MORAINE_MAX_CONCURRENT_SYNCS", default_value_t = 4)]
	pub max_concurrent_syncs: u32,

	#[arg(long, env = "MORAINE_MAINTENANCE_INTERVAL_SECONDS", default_value_t = 3_600)]
	pub maintenance_interval_seconds: u64,

	#[arg(long, env = "MORAINE_TLS_EXTRA_ROOTS")]
	pub tls_extra_roots: Option<PathBuf>,

	#[arg(long, env = "MORAINE_SKIP_MIGRATE_ON_START", default_value_t = false)]
	pub skip_migrate_on_start: bool,

	#[arg(long, env = "MORAINE_DATABASE_URL")]
	pub database_url: Option<String>,

	#[arg(long, env = "MORAINE_MAX_FEED_SCAN_PAGES", default_value_t = 50)]
	pub max_feed_scan_pages: u32,

	#[arg(long, env = "MORAINE_SCANNER_ENABLED", default_value_t = false)]
	pub scanner_enabled: bool,

	#[arg(long, env = "MORAINE_SCANNER_PROVIDER_ID", default_value = "local-clamav")]
	pub scanner_provider_id: String,

	#[arg(long, env = "MORAINE_SCANNER_KIND", default_value = "clamav")]
	pub scanner_kind: String,

	#[arg(long, env = "MORAINE_SCANNER_COMMAND", default_value = "clamscan")]
	pub scanner_command: String,

	#[arg(long, env = "MORAINE_SCANNER_ARGS", value_delimiter = ',')]
	pub scanner_args: Vec<String>,

	#[arg(long, env = "MORAINE_SCANNER_TIMEOUT_SECONDS", default_value_t = 300)]
	pub scanner_timeout_seconds: u64,

	#[arg(long, env = "MORAINE_PUBLISHING", value_enum, default_value_t = Publishing::Review)]
	pub publishing: Publishing,

	#[arg(long, env = "MORAINE_REGISTRATION", value_enum, default_value_t = Registration::Closed)]
	pub registration: Registration,

	#[arg(long, env = "MORAINE_FEDERATION_ALLOW_HTTP_LOCAL", default_value_t = false)]
	pub allow_insecure_federation_local: bool,

	#[arg(long, env = "MORAINE_WEB_DIR")]
	pub web_dir: Option<PathBuf>,

	#[command(flatten)]
	pub s3: S3Settings,
}

#[derive(Debug, Clone, Default, Args)]
pub struct S3Settings {
	#[arg(long, env = "MORAINE_S3_BUCKET")]
	pub bucket: Option<String>,

	#[arg(long, env = "MORAINE_S3_ENDPOINT")]
	pub endpoint: Option<String>,

	#[arg(long, env = "MORAINE_S3_REGION")]
	pub region: Option<String>,

	#[arg(long, env = "MORAINE_S3_ACCESS_KEY_ID")]
	pub access_key_id: Option<String>,

	#[arg(long, env = "MORAINE_S3_SECRET_ACCESS_KEY")]
	pub secret_access_key: Option<String>,

	#[arg(long, env = "MORAINE_S3_PREFIX", default_value = "moraine")]
	pub prefix: String,
}

impl Config {
	pub fn check_binding(&self) -> Result<(), String> {
		if self.bind.ip().is_loopback() || self.tls_terminated || self.allow_insecure_http {
			return Ok(());
		}
		Err(format!(
			"binding {} serves plain HTTP; put a TLS-terminating reverse proxy in front and pass --tls-terminated, or set MORAINE_ALLOW_INSECURE_HTTP=true to accept the risk",
			self.bind
		))
	}

	pub fn database_url(&self) -> String {
		self.database_url
			.clone()
			.unwrap_or_else(|| crate::db::sqlite_url(&self.data_dir.join("metadata.sqlite")))
	}

	pub fn uses_sqlite(&self) -> bool {
		self.database_url().starts_with("sqlite:")
	}

	pub fn database_label(&self) -> String {
		let url = self.database_url();
		if let (Some(scheme), Some(at)) = (url.find("://"), url.find('@'))
			&& at > scheme + 3
			&& let Some((user, _)) = url[scheme + 3..at].split_once(':')
		{
			return format!("{}://{user}:***@{}", &url[..scheme], &url[at + 1..]);
		}
		url
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn config(database_url: Option<String>) -> Config {
		Config {
			bind: "127.0.0.1:0".parse().expect("addr"),
			data_dir: std::path::PathBuf::from("/tmp/moraine"),
			max_artifact_bytes: 1024,
			max_upload_bytes_per_account: 5_368_709_120,
			max_projects: 10_000,
			tls_terminated: false,
			allow_insecure_http: false,
			web_origins: Vec::new(),
			registration: crate::config::Registration::Open,
			max_definitions: 1_000,
			max_sync_entries: 10_000,
			metrics_token: None,
			smtp_url: None,
			mail_from: None,
			public_url: None,
			require_verified_email: false,
			max_mirror_probes_per_cycle: 20,
			max_mirror_probe_bytes: 268_435_456,
			max_feed_page_entries: 100,
			max_feed_scan_pages: 50,
			scanner_enabled: false,
			scanner_provider_id: "local-clamav".to_string(),
			scanner_kind: "clamav".to_string(),
			scanner_command: "clamscan".to_string(),
			scanner_args: Vec::new(),
			scanner_timeout_seconds: 300,
			skip_migrate_on_start: false,
			database_url,
			max_response_bytes: 16_777_216,
			staging_retention_seconds: 3_600,
			blob_retention_seconds: 604_800,
			max_sync_pages: 200,
			requests_per_minute: 600,
			max_concurrent_syncs: 4,
			maintenance_interval_seconds: 3_600,
			tls_extra_roots: None,
			allow_insecure_federation_local: false,
			publishing: Publishing::Review,
			web_dir: None,
			s3: Default::default(),
		}
	}

	#[test]
	fn hides_a_password_in_the_database_label() {
		let labelled = config(Some("postgres://app:secret@host:5432/moraine".to_string())).database_label();
		assert_eq!(labelled, "postgres://app:***@host:5432/moraine");
	}

	#[test]
	fn leaves_a_credential_free_url_alone() {
		let labelled = config(Some("postgres://host/moraine".to_string())).database_label();
		assert_eq!(labelled, "postgres://host/moraine");
	}

	#[test]
	fn refuses_a_public_bind_without_tls() {
		let mut config = config(None);
		config.bind = "0.0.0.0:8080".parse().expect("addr");
		assert!(config.check_binding().is_err());

		config.tls_terminated = true;
		assert!(config.check_binding().is_ok());

		config.tls_terminated = false;
		config.allow_insecure_http = true;
		assert!(config.check_binding().is_ok());
	}

	#[test]
	fn allows_loopback_without_tls() {
		assert!(config(None).check_binding().is_ok());
	}

	#[test]
	fn labels_the_default_sqlite_store() {
		let labelled = config(None).database_label();
		assert!(labelled.starts_with("sqlite:"), "{labelled}");
	}
}
