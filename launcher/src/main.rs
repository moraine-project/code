use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use moraine_launcher::{FilesystemBlobs, apply_install, plan_install};

#[derive(Parser)]
#[command(
	name = "moraine-launcher",
	about = "Verify a lockfile's artifacts and apply a game install plan"
)]
struct Cli {
	#[command(subcommand)]
	command: Command,
}

#[derive(Subcommand)]
enum Command {
	Install {
		#[arg(long)]
		lockfile: PathBuf,

		#[arg(long)]
		blobs: Option<PathBuf>,

		#[arg(long)]
		home: Option<String>,
		#[arg(long, default_value_t = false)]
		allow_http_local: bool,

		#[arg(long)]
		root: PathBuf,
		#[arg(long)]
		adapter: String,

		#[arg(long, default_value_t = false)]
		dry_run: bool,
	},

	Resolve {
		#[arg(long)]
		home: String,
		#[arg(long)]
		project: String,
		#[arg(long)]
		game: String,
		#[arg(long)]
		game_version: String,
		#[arg(long)]
		loader: Option<String>,
		#[arg(long)]
		loader_version: Option<String>,
		#[arg(long)]
		runtime: Option<String>,
		#[arg(long)]
		runtime_version: Option<String>,
		#[arg(long, default_value = "client")]
		side: String,
		#[arg(long, default_value_t = false)]
		allow_http_local: bool,
		#[arg(long)]
		previous: Option<PathBuf>,
		#[arg(long)]
		output: Option<PathBuf>,
	},
}

fn main() -> std::process::ExitCode {
	match run(Cli::parse()) {
		Ok(()) => std::process::ExitCode::SUCCESS,
		Err(message) => {
			eprintln!("error: {message}");
			std::process::ExitCode::FAILURE
		}
	}
}

fn run(cli: Cli) -> Result<(), String> {
	match cli.command {
		Command::Install {
			lockfile,
			blobs,
			home,
			allow_http_local,
			root,
			adapter,
			dry_run,
		} => install(
			&lockfile,
			blobs.as_deref(),
			home.as_deref(),
			allow_http_local,
			&root,
			&adapter,
			dry_run,
		),
		Command::Resolve {
			home,
			project,
			game,
			game_version,
			loader,
			loader_version,
			runtime,
			runtime_version,
			side,
			allow_http_local,
			previous,
			output,
		} => resolve(
			&home,
			&project,
			&game,
			&game_version,
			loader.map(|id| (id, loader_version)),
			runtime.map(|id| (id, runtime_version)),
			&side,
			allow_http_local,
			previous.as_deref(),
			output.as_deref(),
		),
	}
}

#[allow(clippy::too_many_arguments)]
fn resolve(
	home: &str,
	project: &str,
	game: &str,
	game_version: &str,
	loader: Option<(String, Option<String>)>,
	runtime: Option<(String, Option<String>)>,
	side: &str,
	allow_http_local: bool,
	previous: Option<&Path>,
	output: Option<&Path>,
) -> Result<(), String> {
	let source = moraine_launcher::catalog::HttpHome::new(home, allow_http_local)?;
	let request = moraine_launcher::catalog::request_for(game, game_version, project, side, loader, runtime)?;
	let previous = match previous {
		Some(path) => Some(read_lockfile(path)?),
		None => None,
	};
	let lockfile = moraine_launcher::catalog::resolve_from_home(&source, &request, previous.as_ref())?;
	let json = serde_json::to_string_pretty(&lockfile).map_err(|error| error.to_string())?;
	match output {
		Some(path) => std::fs::write(path, format!("{json}\n")).map_err(|error| format!("{}: {error}", path.display())),
		None => {
			println!("{json}");
			Ok(())
		}
	}
}

fn read_lockfile(lockfile_path: &Path) -> Result<moraine_resolver::Lockfile, String> {
	let text = std::fs::read_to_string(lockfile_path).map_err(|error| format!("{}: {error}", lockfile_path.display()))?;
	serde_json::from_str(&text).map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
fn install(
	lockfile_path: &Path,
	blobs: Option<&Path>,
	home: Option<&str>,
	allow_http_local: bool,
	root: &Path,
	adapter: &str,
	dry_run: bool,
) -> Result<(), String> {
	let lockfile = read_lockfile(lockfile_path)?;
	let source: Box<dyn moraine_launcher::BlobSource> = match (blobs, home) {
		(Some(directory), _) => Box::new(FilesystemBlobs::new(directory)),
		(None, Some(url)) => Box::new(moraine_launcher::HttpBlobs::new(url, allow_http_local)?),
		(None, None) => return Err("pass --blobs <dir> or --home <url>".to_string()),
	};
	let prepared = plan_install(&lockfile, source.as_ref(), adapter, &mut moraine_launcher::NoProgress)
		.map_err(|error| error.to_string())?;
	println!("verified {} artifact(s)", prepared.report.verified);
	if dry_run {
		for placement in &prepared.report.placements {
			println!(
				"would place {} <- sha256:{}",
				placement.relative_path.display(),
				hex::encode(placement.digest)
			);
		}
		return Ok(());
	}
	let written = apply_install(&prepared, root, &mut moraine_launcher::NoProgress).map_err(|error| error.to_string())?;
	for path in written {
		println!("wrote {}", path.display());
	}
	Ok(())
}
