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
	/// Verify a lockfile against a blob directory and install under a root.
	Install {
		#[arg(long)]
		lockfile: PathBuf,
		/// Directory of blobs named by their sha256 hex digest.
		#[arg(long)]
		blobs: Option<PathBuf>,
		/// Home registry to fetch blobs from instead of a local directory.
		#[arg(long)]
		home: Option<String>,
		#[arg(long, default_value_t = false)]
		allow_http_local: bool,
		/// Instance root the adapter places files under.
		#[arg(long)]
		root: PathBuf,
		#[arg(long)]
		adapter: String,
		/// Verify and print the plan without writing anything.
		#[arg(long, default_value_t = false)]
		dry_run: bool,
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
	}
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
	let text = std::fs::read_to_string(lockfile_path).map_err(|error| format!("{}: {error}", lockfile_path.display()))?;
	let lockfile: moraine_resolver::Lockfile = serde_json::from_str(&text).map_err(|error| error.to_string())?;
	let source: Box<dyn moraine_launcher::BlobSource> = match (blobs, home) {
		(Some(directory), _) => Box::new(FilesystemBlobs::new(directory)),
		(None, Some(url)) => Box::new(moraine_launcher::HttpBlobs::new(url, allow_http_local)?),
		(None, None) => return Err("pass --blobs <dir> or --home <url>".to_string()),
	};
	let prepared = plan_install(&lockfile, source.as_ref(), adapter).map_err(|error| error.to_string())?;
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
	let written = apply_install(&prepared, root).map_err(|error| error.to_string())?;
	for path in written {
		println!("wrote {}", path.display());
	}
	Ok(())
}
