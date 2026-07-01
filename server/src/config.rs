use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;

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
}
