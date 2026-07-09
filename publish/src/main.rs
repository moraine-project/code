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
	/// Generate a publisher key file.
	Keygen {
		#[arg(long, default_value = "publisher.key")]
		key: PathBuf,
	},
	/// Create a project from a fresh genesis and import it at a home.
	Init {
		#[arg(long)]
		key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		home_hint: Option<String>,
	},
	/// Sign a release for a local artifact and store it at a home.
	Release {
		#[arg(long)]
		key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		project: String,
		#[arg(long)]
		game: String,
		/// A game version this release targets. Repeatable.
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
	},
	/// Sign a withdrawal for a release; the release record stays intact.
	Withdraw {
		#[arg(long)]
		key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		project: String,
		#[arg(long)]
		release: String,
		/// One of compromise, harmful, broken, legal, author-preference.
		#[arg(long)]
		reason: String,
		#[arg(long)]
		note: Option<String>,
	},
	/// Transfer project ownership; needs the previous and new owner keys.
	Transfer {
		#[arg(long)]
		key: PathBuf,
		#[arg(long = "cosign-key")]
		cosign_key: PathBuf,
		#[arg(long)]
		home: String,
		#[arg(long)]
		project: String,
		/// Previous owner as `user:<id>` or `org:<id>`.
		#[arg(long)]
		from: String,
		/// New owner as `user:<id>` or `org:<id>`.
		#[arg(long)]
		to: String,
	},
	/// Upload an artifact to a home's blob store.
	Upload {
		#[arg(long)]
		home: String,
		#[arg(long)]
		file: PathBuf,
		#[arg(long, env = "MORAINE_API_KEY")]
		api_key: Option<String>,
	},
	/// Sign and store a project profile revision.
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
	/// Append a feed entry that publishes a stored object.
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
	/// Submit a signed feed entry for admission review.
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
