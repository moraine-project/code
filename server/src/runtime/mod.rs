mod commands;
mod server;
mod workers;

use crate::config::Cli;

pub async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
	let config = cli.config;
	if config.skip_migrate_on_start {
		match crate::db::migrations::pending_url(&config.database_url()).await {
			Ok(0) => {}
			Ok(pending) => {
				return Err(format!("{pending} migration(s) pending; run `moraine-server migrate`").into());
			}
			Err(error) => return Err(error.into()),
		}
	}
	if let Some(command) = cli.command {
		return commands::run(&config, command).await;
	}
	server::run(config).await
}
