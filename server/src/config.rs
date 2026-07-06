use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Publishing {
	Review,
	Open,
}

impl Publishing {
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Review => "review",
			Self::Open => "open",
		}
	}
}

#[derive(Debug, Clone, Parser)]
#[command(name = "moraine-server", about = "Moraine registry, directory, and worker")]
pub struct Config {
	#[arg(long, env = "MORAINE_BIND", default_value = "127.0.0.1:8080")]
	pub bind: SocketAddr,

	#[arg(long, env = "MORAINE_DATA_DIR", default_value = "./data")]
	pub data_dir: PathBuf,

	#[arg(long, env = "MORAINE_MAX_ARTIFACT_BYTES", default_value_t = 536_870_912)]
	pub max_artifact_bytes: u64,

	#[arg(long, env = "MORAINE_MAX_FEED_PAGE_ENTRIES", default_value_t = 100)]
	pub max_feed_page_entries: u32,

	#[arg(long, env = "MORAINE_PUBLISHING", value_enum, default_value_t = Publishing::Review)]
	pub publishing: Publishing,

	#[arg(long, env = "MORAINE_FEDERATION_ALLOW_HTTP_LOCAL", default_value_t = false)]
	pub allow_insecure_federation_local: bool,

	/// Directory of a built website to serve from the same origin.
	#[arg(long, env = "MORAINE_WEB_DIR")]
	pub web_dir: Option<PathBuf>,
}
