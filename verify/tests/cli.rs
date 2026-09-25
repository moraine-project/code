use std::path::Path;
use std::process::{Command, Output};

use moraine_crypto::{ObjectKind, SigningKey, object_id_string};
use moraine_model::genesis::{Genesis, GenesisKind, RootKey};
use moraine_model::signed::sign_payload;

fn signed_genesis(key: &SigningKey) -> Vec<u8> {
	let genesis = Genesis {
		protocol: 1,
		kind: GenesisKind::Project,
		nonce: vec![0x11; 16],
		roots: vec![RootKey::from_public_key(key.verifying_key().to_bytes().to_vec()).expect("valid root key")],
		threshold: 1,
		authorized_kinds: vec!["delegation".to_string(), "release".to_string(), "profile".to_string()],
		home_hint: None,
		contacts: None,
		created_at: 1_760_000_000,
	};
	sign_payload(ObjectKind::Genesis, &genesis, &[key])
		.expect("valid signed genesis")
		.wire_bytes()
}

fn moraine_verify(arguments: &[&str]) -> Output {
	Command::new(env!("CARGO_BIN_EXE_moraine-verify"))
		.args(arguments)
		.output()
		.expect("the verifier runs")
}

#[test]
fn a_rootless_verification_exits_nonzero_while_a_rooted_one_exits_zero() {
	let directory = tempfile::tempdir().expect("tempdir");
	let key = SigningKey::from_seed(&[0x21; 32]);
	let path = directory.path().join("genesis");
	let wire = signed_genesis(&key);
	std::fs::write(&path, &wire).expect("write");
	let file = path.to_str().expect("path");
	let root = hex::encode(key.verifying_key().to_bytes());
	let payload = moraine_model::signed::SignedObject::<Genesis>::from_bytes(&wire)
		.expect("the fixture decodes")
		.payload_bytes;
	let id = object_id_string(ObjectKind::Genesis, &payload);

	let rootless = moraine_verify(&["object", "--kind", "genesis", file]);
	assert!(
		!rootless.status.success(),
		"verifying without a root must not report success: {}",
		String::from_utf8_lossy(&rootless.stdout)
	);
	let stderr = String::from_utf8_lossy(&rootless.stderr);
	assert!(stderr.contains("no trusted roots"), "{stderr}");
	assert!(stderr.contains("--root"), "{stderr}");

	let wrong_root = moraine_verify(&["object", "--kind", "genesis", file, "--root", &hex::encode([9u8; 32])]);
	assert!(!wrong_root.status.success(), "an untrusted root must not verify");

	let verified = moraine_verify(&["object", "--kind", "genesis", file, "--root", &root]);
	assert!(
		verified.status.success(),
		"a valid root must still verify: {}",
		String::from_utf8_lossy(&verified.stderr)
	);
	let stdout = String::from_utf8_lossy(&verified.stdout);
	assert!(stdout.contains(&id), "{stdout}");
	assert!(stdout.contains("signatures: ok"), "{stdout}");
}

#[test]
fn only_decode_only_may_report_an_unverified_object_as_a_success() {
	let directory = tempfile::tempdir().expect("tempdir");
	let key = SigningKey::from_seed(&[0x33; 32]);
	let path = directory.path().join("genesis");
	std::fs::write(&path, signed_genesis(&key)).expect("write");
	let file = path.to_str().expect("path");

	let decoded = moraine_verify(&["object", "--kind", "genesis", file, "--decode-only"]);
	assert!(
		decoded.status.success(),
		"--decode-only is the documented opt-in: {}",
		String::from_utf8_lossy(&decoded.stderr)
	);
	let stdout = String::from_utf8_lossy(&decoded.stdout);
	assert!(stdout.contains("not checked"), "{stdout}");

	let artifact = moraine_verify(&["artifact", "--release", file, "--file", file]);
	assert!(
		!artifact.status.success(),
		"an artifact check without a root must not report success: {}",
		String::from_utf8_lossy(&artifact.stdout)
	);
}

#[test]
fn a_missing_object_never_verifies() {
	let missing = Path::new("/nonexistent/moraine-verify-regression");
	let output = moraine_verify(&["object", "--kind", "genesis", &missing.to_string_lossy()]);
	assert!(!output.status.success(), "a file that cannot be read is not a verification");
}
