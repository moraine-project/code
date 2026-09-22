mod auth;
mod blob;
mod capability;
mod config;
mod db;
mod federation;
mod mail;
mod ops;
mod registry;
mod routes;
mod runtime;
#[cfg(test)]
mod tests;
mod verify;

use clap::Parser;
#[cfg(test)]
pub(crate) use tests::support as test_support;
use tracing_subscriber::EnvFilter;

use crate::config::Cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	tracing_subscriber::fmt()
		.with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
		.init();

	runtime::run(Cli::parse()).await
}
