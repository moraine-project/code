use std::path::Path;

use moraine_crypto::ObjectKind;
use moraine_model::release::ReleaseObject;
use moraine_model::signed::{SignedObject, TrustedKey};
use sha2::{Digest, Sha256};

pub struct ArtifactVerdict {
	pub human_version: String,
	pub channel: String,
	pub digest: [u8; 32],
	pub filename: String,
	pub size: u64,
	pub is_primary: bool,
}

pub fn verify(
	release_path: &Path,
	artifact_path: &Path,
	roots: &[String],
	threshold: usize,
) -> Result<ArtifactVerdict, String> {
	if roots.is_empty() {
		return Err("at least one --root is required".to_string());
	}
	let wire = std::fs::read(release_path).map_err(|error| format!("{}: {error}", release_path.display()))?;
	let signed = SignedObject::<ReleaseObject>::from_bytes(&wire).map_err(|error| error.to_string())?;
	let trusted = roots
		.iter()
		.map(|root| {
			let bytes = hex::decode(root).map_err(|_| format!("`{root}` is not hex"))?;
			TrustedKey::new(&bytes).map_err(|error| error.to_string())
		})
		.collect::<Result<Vec<_>, _>>()?;
	signed
		.verify_threshold(ObjectKind::Release, &trusted, threshold)
		.map_err(|error| error.to_string())?;
	let ReleaseObject::Release(release) = signed.payload else {
		return Err("object is not a release".to_string());
	};

	let digest = hash_file(artifact_path)?;
	let size = std::fs::metadata(artifact_path)
		.map_err(|error| format!("{}: {error}", artifact_path.display()))?
		.len();
	let artifact = release
		.artifacts
		.iter()
		.find(|artifact| artifact.digest == digest)
		.ok_or_else(|| "file digest matches no artifact in the release".to_string())?;
	if artifact.size != size {
		return Err(format!("file is {size} bytes but the release records {}", artifact.size));
	}
	Ok(ArtifactVerdict {
		human_version: release.human_version,
		channel: release.channel,
		digest,
		filename: artifact.filename.clone(),
		size,
		is_primary: artifact.is_primary,
	})
}

fn hash_file(path: &Path) -> Result<[u8; 32], String> {
	use std::io::Read;
	let mut file = std::fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
	let mut hasher = Sha256::new();
	let mut buffer = vec![0u8; 64 * 1024];
	loop {
		let read = file
			.read(&mut buffer)
			.map_err(|error| format!("{}: {error}", path.display()))?;
		if read == 0 {
			break;
		}
		hasher.update(&buffer[..read]);
	}
	Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
	use moraine_crypto::{ObjectKind as Kind, SigningKey};
	use moraine_model::artifact::Artifact;
	use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
	use moraine_model::release::ReleasePayload;
	use moraine_model::signed::sign_payload;

	use super::*;

	fn release_wire(signer: &SigningKey, digest: [u8; 32], size: u64) -> Vec<u8> {
		let release = ReleasePayload {
			protocol: 1,
			project_id: "gd:sha256:p".to_string(),
			game_id: "gd:sha256:g".to_string(),
			release_nonce: vec![0x42; 16],
			human_version: "1.0.0".to_string(),
			channel: "release".to_string(),
			kind: "mod".to_string(),
			declared_time: 1_760_000_000,
			compatibility: vec![Compatibility {
				game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
				loader_id: None,
				loader_version_predicate: None,
				side: Side::Both,
				runtime_predicate: None,
				os_predicate: None,
				arch_predicate: None,
			}],
			artifacts: vec![Artifact {
				digest: digest.to_vec(),
				size,
				media_type: "application/java-archive".to_string(),
				filename: "mod.jar".to_string(),
				is_primary: true,
				os_predicate: None,
				arch_predicate: None,
			}],
			dependencies: Vec::new(),
			source_reference: None,
			changelog_digest: None,
			license_expression: None,
			rights: None,
			sbom_digest: None,
			minimum_verifier_version: 1,
			critical_extensions: Vec::new(),
		};
		sign_payload(Kind::Release, &release, &[signer]).wire_bytes()
	}

	#[test]
	fn verifies_a_file_and_rejects_a_tampered_one() {
		let directory = tempfile::tempdir().expect("tempdir");
		let signer = SigningKey::from_seed(&[1u8; 32]);
		let bytes = b"mod bytes";
		let digest: [u8; 32] = Sha256::digest(bytes).into();
		let release_path = directory.path().join("release.cbor");
		std::fs::write(&release_path, release_wire(&signer, digest, bytes.len() as u64)).expect("release");
		let file_path = directory.path().join("mod.jar");
		std::fs::write(&file_path, bytes).expect("artifact");
		let root = hex::encode(signer.verifying_key().to_bytes());

		let verdict = verify(&release_path, &file_path, std::slice::from_ref(&root), 1).expect("verified");
		assert_eq!(verdict.human_version, "1.0.0");
		assert_eq!(verdict.filename, "mod.jar");
		assert!(verdict.is_primary);

		std::fs::write(&file_path, b"tampered bytes").expect("tamper");
		assert!(verify(&release_path, &file_path, &[root], 1).is_err());
	}
}
