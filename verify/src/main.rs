mod artifact;
mod definitions;
mod generate;
mod records;
mod vector;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use moraine_crypto::{ObjectKind, object_id_string};
use moraine_model::Canonical;
use moraine_model::advisory::Advisory;
use moraine_model::attestation::AttestationObject;
use moraine_model::changelog::Changelog;
use moraine_model::definition::{GameDef, LoaderObject, RuntimeDef};
use moraine_model::delegation::Delegation;
use moraine_model::deny_list::DenyList;
use moraine_model::feed::FeedEntry;
use moraine_model::genesis::Genesis;
use moraine_model::modpack::ModpackManifest;
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
		#[arg(long)]
		decode_only: bool,
	},

	Artifact {
		#[arg(long)]
		release: PathBuf,
		#[arg(long)]
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
			decode_only,
		} => verify_object(&kind, &file, &roots, threshold, decode_only),
		Command::Artifact {
			release,
			file,
			roots,
			threshold,
		} => verify_artifact(&release, &file, &roots, threshold),
	}
}

fn verify_artifact(release: &Path, file: &Path, roots: &[String], threshold: usize) -> Result<(), String> {
	let verdict = artifact::verify(release, file, roots, threshold)?;
	println!("release: {} ({})", verdict.human_version, verdict.channel);
	println!("file: {} (sha256:{})", file.display(), hex::encode(verdict.digest));
	println!(
		"artifact: {}{}",
		verdict.filename,
		if verdict.is_primary { " (primary)" } else { "" }
	);
	println!("size: {} bytes", verdict.size);
	println!("status: verified");
	Ok(())
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

fn verify_object(kind: &str, file: &PathBuf, roots: &[String], threshold: usize, decode_only: bool) -> Result<(), String> {
	if roots.is_empty() && !decode_only {
		return Err(
			"no trusted roots supplied, so no signature was checked; pass --root <public-key-hex> to verify, or --decode-only to inspect the bytes without trusting them"
				.to_string(),
		);
	}
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
		ObjectKind::Modpack => describe::<ModpackManifest>(kind, &bytes)?,
		ObjectKind::Changelog => describe::<Changelog>(kind, &bytes)?,
		ObjectKind::DenyList => describe::<DenyList>(kind, &bytes)?,
	};
	println!("id: {id}");
	if trusted.is_empty() {
		println!("signatures: not checked (--decode-only)");
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

	#[test]
	fn verifies_changelog_and_modpack_objects() {
		use moraine_crypto::{ObjectKind, SigningKey};
		use moraine_model::changelog::{Changelog, ChangelogSection, LocaleSection};
		use moraine_model::compatibility::Side;
		use moraine_model::dependency::TargetKind;
		use moraine_model::modpack::{ModpackEntry, ModpackManifest};
		use moraine_model::signed::sign_payload;

		let key = SigningKey::from_seed(&[0x5A; 32]);
		let root = hex::encode(key.verifying_key().to_bytes());
		let directory = tempfile::tempdir().expect("tempdir");

		let changelog = Changelog {
			protocol: 1,
			project_id: "gd:sha256:aa".to_string(),
			release_id: None,
			locale_sections: vec![LocaleSection {
				locale: "en".to_string(),
				sections: vec![ChangelogSection {
					heading: "Fixes".to_string(),
					body: "A fix".to_string(),
					severity: None,
				}],
			}],
			declared_time: 1_760_000_000,
		};
		let changelog_path = directory.path().join("changelog");
		std::fs::write(
			&changelog_path,
			sign_payload(ObjectKind::Changelog, &changelog, &[&key]).wire_bytes(),
		)
		.expect("write");
		super::verify_object("changelog", &changelog_path, std::slice::from_ref(&root), 1, false)
			.expect("changelog verifies");

		let modpack = ModpackManifest {
			protocol: 1,
			project_id: "gd:sha256:aa".to_string(),
			game_id: "gd:sha256:bb".to_string(),
			loader_id: None,
			entries: vec![ModpackEntry {
				ordinal: 0,
				target_kind: TargetKind::Project,
				target_id: "gd:sha256:cc".to_string(),
				release_id: "gd:sha256:dd".to_string(),
				digest: vec![0x11; 32],
				applies_to: Side::Both,
			}],
			overrides: Vec::new(),
			server_manifest_digest: None,
			declared_time: 1_760_000_000,
		};
		let modpack_path = directory.path().join("modpack");
		std::fs::write(
			&modpack_path,
			sign_payload(ObjectKind::Modpack, &modpack, &[&key]).wire_bytes(),
		)
		.expect("write");
		super::verify_object("modpack", &modpack_path, &[root], 1, false).expect("modpack verifies");
	}

	#[test]
	fn search_fixture_merges_by_project_id() {
		#[derive(serde::Deserialize)]
		struct Fixture {
			responses: Vec<moraine_model::search::SearchResponse>,
		}

		let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../protocol/vectors/search.json");
		let text = std::fs::read_to_string(path).expect("search.json is committed");
		let fixture: Fixture = serde_json::from_str(&text).expect("search.json parses");
		for response in &fixture.responses {
			response.validate().expect("fixture responses are valid");
		}
		let merged = moraine_model::search::merge(fixture.responses);
		assert_eq!(merged.results.len(), 2);
		let shared = merged
			.results
			.iter()
			.find(|result| result.listings.len() == 2)
			.expect("one merged project");
		assert!(
			shared
				.listings
				.iter()
				.any(|listing| listing.source_instance == "https://dir-b.example")
		);
	}

	#[test]
	fn an_object_without_roots_is_a_refusal_not_a_pass() {
		let directory = tempfile::tempdir().expect("tempdir");
		let path = directory.path().join("object");
		std::fs::write(&path, b"not really an object").expect("write");

		let refusal =
			super::verify_object("changelog", &path, &[], 1, false).expect_err("a rootless verification must not succeed");
		assert!(refusal.contains("no trusted roots"), "{refusal}");
		assert!(refusal.contains("--decode-only"), "{refusal}");
	}
}
