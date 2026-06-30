mod definitions;
mod generate;
mod records;
mod vector;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use moraine_crypto::{ObjectKind, object_id_string};
use moraine_model::Canonical;
use moraine_model::advisory::Advisory;
use moraine_model::attestation::AttestationObject;
use moraine_model::definition::{GameDef, LoaderObject, RuntimeDef};
use moraine_model::delegation::Delegation;
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::Genesis;
use moraine_model::profile::ProfileRevision;
use moraine_model::release::ReleaseObject;
use moraine_model::signed::{SignedObject, TrustedKey, verify_envelope};
use sha2::{Digest, Sha256};

#[derive(Parser)]
#[command(name = "moraine-verify", about = "Verify Moraine signed objects and protocol test vectors")]
struct Cli {
	#[command(subcommand)]
	command: Command,
}

#[derive(Subcommand)]
enum Command {
	Vectors {
		#[arg(long, default_value = "protocol/vectors/vectors.json")]
		file: PathBuf,
	},

	GenVectors {
		#[arg(long, default_value = "protocol/vectors/vectors.json")]
		file: PathBuf,
	},

	Hash {
		file: PathBuf,
	},

	KeyId {
		#[arg(long)]
		public: String,
	},

	Object {
		#[arg(long)]
		kind: String,
		file: PathBuf,

		#[arg(long = "root")]
		roots: Vec<String>,
		#[arg(long, default_value_t = 1)]
		threshold: usize,
	},
}

fn main() -> std::process::ExitCode {
	let cli = Cli::parse();
	match run(cli) {
		Ok(()) => std::process::ExitCode::SUCCESS,
		Err(message) => {
			eprintln!("error: {message}");
			std::process::ExitCode::FAILURE
		}
	}
}

fn run(cli: Cli) -> Result<(), String> {
	match cli.command {
		Command::Vectors { file } => run_vectors(&file),
		Command::GenVectors { file } => regenerate(&file),
		Command::Hash { file } => print_hash(&file),
		Command::KeyId { public } => print_key_id(&public),
		Command::Object {
			kind,
			file,
			roots,
			threshold,
		} => verify_object(&kind, &file, &roots, threshold),
	}
}

fn run_vectors(file: &PathBuf) -> Result<(), String> {
	let text = std::fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
	let corpus: vector::VectorFile = serde_json::from_str(&text).map_err(|error| error.to_string())?;
	let mut failures = Vec::new();
	for case in &corpus.vectors {
		if let Err(message) = vector::check(case) {
			failures.push(format!("{}: {message}", case.name));
		}
	}
	let total = corpus.vectors.len();
	if failures.is_empty() {
		println!("ok: {total} vectors passed");
		return Ok(());
	}
	for failure in &failures {
		eprintln!("FAIL {failure}");
	}
	Err(format!("{} of {total} vectors failed", failures.len()))
}

fn regenerate(file: &PathBuf) -> Result<(), String> {
	let corpus = generate::generate();
	let text = serde_json::to_string_pretty(&corpus).map_err(|error| error.to_string())?;
	std::fs::write(file, format!("{text}\n")).map_err(|error| format!("{}: {error}", file.display()))?;
	println!("wrote {} vectors to {}", corpus.vectors.len(), file.display());
	Ok(())
}

fn print_hash(file: &PathBuf) -> Result<(), String> {
	let bytes = std::fs::read(file).map_err(|error| format!("{}: {error}", file.display()))?;
	println!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
	Ok(())
}

fn print_key_id(public: &str) -> Result<(), String> {
	let bytes = hex::decode(public).map_err(|error| error.to_string())?;
	let key = TrustedKey::new(&bytes).map_err(|error| error.to_string())?;
	println!("{}", key.key_id);
	Ok(())
}

fn verify_object(kind: &str, file: &PathBuf, roots: &[String], threshold: usize) -> Result<(), String> {
	let kind = ObjectKind::parse(kind).ok_or_else(|| format!("unknown object kind `{kind}`"))?;
	let bytes = std::fs::read(file).map_err(|error| format!("{}: {error}", file.display()))?;
	let trusted: Vec<TrustedKey> = roots
		.iter()
		.map(|root| {
			let bytes = hex::decode(root).map_err(|error| error.to_string())?;
			TrustedKey::new(&bytes).map_err(|error| error.to_string())
		})
		.collect::<Result<_, _>>()?;

	let (id, message) = match kind {
		ObjectKind::Genesis => describe::<Genesis>(kind, &bytes)?,
		ObjectKind::Delegation => describe::<Delegation>(kind, &bytes)?,
		ObjectKind::Release => describe::<ReleaseObject>(kind, &bytes)?,
		ObjectKind::Attestation => describe::<AttestationObject>(kind, &bytes)?,
		ObjectKind::Advisory => describe::<Advisory>(kind, &bytes)?,
		ObjectKind::FeedEntry => describe::<FeedEntry>(kind, &bytes)?,
		ObjectKind::Profile => describe::<ProfileRevision>(kind, &bytes)?,
		ObjectKind::GameDef => describe::<GameDef>(kind, &bytes)?,
		ObjectKind::LoaderDef => describe::<LoaderObject>(kind, &bytes)?,
		ObjectKind::RuntimeDef => describe::<RuntimeDef>(kind, &bytes)?,
		other => return Err(format!("object kind `{}` is not implemented yet", other.as_str())),
	};
	println!("id: {id}");
	if trusted.is_empty() {
		println!("signatures: not checked (no trusted roots supplied)");
		return Ok(());
	}
	let envelope = decode_envelope(kind, &bytes)?;
	verify_envelope(&envelope, &message, &trusted, threshold).map_err(|error| error.to_string())?;
	println!("signatures: ok ({threshold} of {} required)", trusted.len());
	Ok(())
}

fn describe<T: moraine_model::Canonical>(kind: ObjectKind, bytes: &[u8]) -> Result<(String, Vec<u8>), String> {
	let signed = SignedObject::<T>::from_bytes(bytes).map_err(|error| error.to_string())?;
	Ok((object_id_string(kind, &signed.payload_bytes), signed.signed_message(kind)))
}

fn decode_envelope(kind: ObjectKind, bytes: &[u8]) -> Result<moraine_model::SignatureEnvelope, String> {
	let value = moraine_codec::decode(bytes).map_err(|error| error.to_string())?;
	let envelope = value.get("envelope").ok_or("missing envelope")?.clone();
	let _ = kind;
	moraine_model::SignatureEnvelope::from_value(envelope).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
	#[test]
	fn committed_corpus_passes() {
		let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../protocol/vectors/vectors.json");
		let text = std::fs::read_to_string(path).expect("vectors.json is committed");
		let corpus: crate::vector::VectorFile = serde_json::from_str(&text).expect("vectors.json parses");
		for case in &corpus.vectors {
			crate::vector::check(case).unwrap_or_else(|error| panic!("{}: {error}", case.name));
		}
	}
}
