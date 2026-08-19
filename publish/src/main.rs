mod artifacts;
mod commands;
mod home;
mod keyfile;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "moraine-publish", about = "Sign and publish releases to a Moraine home")]
struct Cli {
	#[command(subcommand)]
	command: Command,
}

#[derive(Subcommand)]
enum Command {
	Keygen {
		#[arg(long, default_value = "publisher.key")]
		key: PathBuf,
	},

	Init {
		#[arg(long)]
		key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		home_hint: Option<String>,
	},

	Release {
		#[arg(long)]
		key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		project: String,
		#[arg(long)]
		game: String,

		#[arg(long = "game-version")]
		game_versions: Vec<String>,
		#[arg(long)]
		version: String,
		#[arg(long, default_value = "release")]
		channel: String,
		#[arg(long)]
		file: PathBuf,
		#[arg(long)]
		loader: Option<String>,
		#[arg(long)]
		changelog: Option<String>,
	},

	Changelog {
		#[arg(long)]
		key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		project: String,
		#[arg(long)]
		release: Option<String>,
		#[arg(long, default_value = "en")]
		locale: String,
		#[arg(long)]
		file: PathBuf,
	},

	Provider {
		#[arg(long)]
		key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		id: String,
		#[arg(long, env = "MORAINE_API_KEY")]
		api_key: Option<String>,
	},

	Advisory {
		#[arg(long)]
		key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		provider: String,
		#[arg(long)]
		project: String,
		#[arg(long)]
		game: String,
		#[arg(long)]
		digest: String,
		#[arg(long)]
		severity: String,
		#[arg(long)]
		category: String,
		#[arg(long, default_value_t = false)]
		block: bool,
		#[arg(long)]
		evidence: Option<String>,
	},

	Withdraw {
		#[arg(long)]
		key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		project: String,
		#[arg(long)]
		release: String,

		#[arg(long)]
		reason: String,
		#[arg(long)]
		note: Option<String>,
	},

	Transfer {
		#[arg(long)]
		key: PathBuf,
		#[arg(long = "cosign-key")]
		cosign_key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		project: String,

		#[arg(long)]
		from: String,

		#[arg(long)]
		to: String,
	},

	Inspect {
		file: PathBuf,
	},

	Plan {
		#[arg(long)]
		adapter: String,

		#[arg(long = "mod")]
		mods: Vec<String>,

		#[arg(long = "override")]
		overrides: Vec<String>,
	},

	Upload {
		#[arg(long)]
		home: String,
		#[arg(long)]
		file: PathBuf,
		#[arg(long, env = "MORAINE_API_KEY")]
		api_key: Option<String>,
	},

	Profile {
		#[arg(long)]
		key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		project: String,
		#[arg(long)]
		game: String,
		#[arg(long)]
		name: String,
		#[arg(long, default_value = "")]
		summary: String,
		#[arg(long, default_value = "")]
		description: String,
		#[arg(long = "category")]
		categories: Vec<String>,
		#[arg(long = "tag")]
		tags: Vec<String>,
	},

	Publish {
		#[arg(long)]
		key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		project: String,
		#[arg(long)]
		object: String,
		#[arg(long, default_value = "release-published")]
		kind: String,
	},

	Submit {
		#[arg(long)]
		key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		project: String,
		#[arg(long)]
		object: String,
		#[arg(long, default_value = "release-published")]
		kind: String,
		#[arg(long, env = "MORAINE_API_KEY")]
		api_key: Option<String>,
	},
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let cli = Cli::parse();
	let result = match cli.command {
		Command::Keygen { key } => commands::keygen(&key),
		Command::Init { key, home, home_hint } => commands::init(&key, &home, home_hint).await,
		Command::Release {
			key,
			home,
			project,
			game,
			game_versions,
			version,
			channel,
			file,
			loader,
			changelog,
		} => {
			commands::release(
				&key,
				&home,
				&project,
				&game,
				&game_versions,
				&version,
				&channel,
				&file,
				loader,
				changelog,
			)
			.await
		}
		Command::Changelog {
			key,
			home,
			project,
			release,
			locale,
			file,
		} => commands::changelog(&key, &home, &project, release, &locale, &file).await,
		Command::Provider { key, home, id, api_key } => commands::provider(&key, &home, &id, api_key).await,
		Command::Advisory {
			key,
			home,
			provider,
			project,
			game,
			digest,
			severity,
			category,
			block,
			evidence,
		} => {
			commands::advisory(
				&key, &home, &provider, &project, &game, &digest, &severity, &category, block, evidence,
			)
			.await
		}
		Command::Withdraw {
			key,
			home,
			project,
			release,
			reason,
			note,
		} => commands::withdraw(&key, &home, &project, &release, &reason, note).await,
		Command::Transfer {
			key,
			cosign_key,
			home,
			project,
			from,
			to,
		} => commands::transfer(&key, &cosign_key, &home, &project, &from, &to).await,
		Command::Inspect { file } => commands::inspect(&file),
		Command::Plan {
			adapter,
			mods,
			overrides,
		} => commands::plan(&adapter, &mods, &overrides),
		Command::Upload { home, file, api_key } => commands::upload(&home, &file, api_key).await,
		Command::Profile {
			key,
			home,
			project,
			game,
			name,
			summary,
			description,
			categories,
			tags,
		} => commands::profile(&key, &home, &project, &game, &name, &summary, &description, categories, tags).await,
		Command::Publish {
			key,
			home,
			project,
			object,
			kind,
		} => commands::publish(&key, &home, &project, &object, &kind).await,
		Command::Submit {
			key,
			home,
			project,
			object,
			kind,
			api_key,
		} => commands::submit(&key, &home, &project, &object, &kind, api_key).await,
	};
	if let Err(message) = result {
		eprintln!("error: {message}");
		std::process::exit(1);
	}
	Ok(())
}
