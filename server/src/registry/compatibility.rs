use crate::db::StoredObject;
use crate::routes::AppState;

pub(crate) fn release_matches_game_version(
	object: &StoredObject,
	version: &str,
	scheme: Option<moraine_model::version::OrderingScheme>,
) -> bool {
	use moraine_model::Canonical;
	let Ok(moraine_model::release::ReleaseObject::Release(release)) =
		moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload)
	else {
		return false;
	};
	release
		.compatibility
		.iter()
		.any(|entry| predicate_satisfied(&entry.game_version_predicate, version, scheme))
}

fn predicate_satisfied(
	predicate: &moraine_model::compatibility::Predicate,
	version: &str,
	scheme: Option<moraine_model::version::OrderingScheme>,
) -> bool {
	use moraine_model::compatibility::{PredicateResult, Scheme};
	use moraine_model::version::{OrderingScheme, VersionCatalog};
	let catalog = match predicate.scheme() {
		Some(Scheme::Any) | Some(Scheme::Exact) | Some(Scheme::Set) => {
			VersionCatalog::new(OrderingScheme::Opaque, Vec::new())
		}
		_ => match scheme {
			Some(scheme) => VersionCatalog::new(scheme, Vec::new()),
			None => return false,
		},
	};
	catalog.evaluate(predicate, version) == PredicateResult::Satisfied
}

pub(crate) fn release_declares_loader(object: &StoredObject, loader_id: &str) -> bool {
	use moraine_model::Canonical;
	let Ok(moraine_model::release::ReleaseObject::Release(release)) =
		moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload)
	else {
		return false;
	};
	release
		.compatibility
		.iter()
		.any(|entry| entry.loader_id.as_deref() == Some(loader_id))
}

pub(crate) fn release_matches_loader_version(
	object: &StoredObject,
	loader_id: &str,
	version: &str,
	scheme: Option<moraine_model::version::OrderingScheme>,
) -> bool {
	use moraine_model::Canonical;
	let Ok(moraine_model::release::ReleaseObject::Release(release)) =
		moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload)
	else {
		return false;
	};
	release.compatibility.iter().any(|entry| {
		if entry.loader_id.as_deref() != Some(loader_id) {
			return false;
		}
		match &entry.loader_version_predicate {
			None => true,
			Some(predicate) => predicate_satisfied(predicate, version, scheme),
		}
	})
}

pub(crate) fn release_matches_runtime(
	object: &StoredObject,
	version: &str,
	scheme: Option<moraine_model::version::OrderingScheme>,
) -> bool {
	use moraine_model::Canonical;
	let Ok(moraine_model::release::ReleaseObject::Release(release)) =
		moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload)
	else {
		return false;
	};
	release.compatibility.iter().any(|entry| match &entry.runtime_predicate {
		None => false,
		Some(predicate) => predicate_satisfied(predicate, version, scheme),
	})
}

pub(crate) async fn runtime_ordering(state: &AppState, runtime_id: &str) -> Option<moraine_model::version::OrderingScheme> {
	use moraine_model::Canonical;
	let definition = state.metadata.definition(runtime_id).await.ok().flatten()?;
	let current = definition.current_digest?;
	let object = state.metadata.object(&current).await.ok().flatten()?;
	let runtime = moraine_model::definition::RuntimeDef::from_canonical_bytes(&object.payload).ok()?;
	moraine_model::version::OrderingScheme::parse(&runtime.version_ordering)
}

pub(crate) async fn loader_ordering(state: &AppState, loader_id: &str) -> Option<moraine_model::version::OrderingScheme> {
	use moraine_model::Canonical;
	let definition = state.metadata.definition(loader_id).await.ok().flatten()?;
	let current = definition.current_digest?;
	let object = state.metadata.object(&current).await.ok().flatten()?;
	let loader = moraine_model::definition::LoaderObject::from_canonical_bytes(&object.payload).ok()?;
	loader_ordering_of(&loader)
}

fn loader_ordering_of(loader: &moraine_model::definition::LoaderObject) -> Option<moraine_model::version::OrderingScheme> {
	match loader {
		moraine_model::definition::LoaderObject::Definition(definition) => definition.ordering(),
		_ => None,
	}
}

pub(crate) async fn game_ordering(
	state: &AppState,
	object: &crate::db::StoredObject,
) -> Option<moraine_model::version::OrderingScheme> {
	use moraine_model::Canonical;
	use moraine_model::version::OrderingScheme;
	let release = moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload)
		.ok()
		.and_then(|release| match release {
			moraine_model::release::ReleaseObject::Release(release) => Some(release),
			_ => None,
		})?;
	let definition = state.metadata.definition(&release.game_id).await.ok().flatten()?;
	let current = definition.current_digest?;
	let object = state.metadata.object(&current).await.ok().flatten()?;
	moraine_model::definition::GameDef::from_canonical_bytes(&object.payload)
		.ok()
		.and_then(|definition| OrderingScheme::parse(&definition.version_ordering))
}

#[cfg(test)]
mod version_tests {
	use moraine_crypto::{ObjectKind, SigningKey};
	use moraine_model::artifact::Artifact;
	use moraine_model::compatibility::{Compatibility, Predicate, Scheme, Side};
	use moraine_model::release::ReleasePayload;
	use moraine_model::signed::sign_payload;
	use moraine_model::version::OrderingScheme;

	use super::*;

	fn release_object(game_predicate: Predicate, loader_predicate: Option<Predicate>) -> StoredObject {
		let signer = SigningKey::from_seed(&[3u8; 32]);
		let release = ReleasePayload {
			protocol: 1,
			project_id: "p".to_string(),
			game_id: "g".to_string(),
			release_nonce: vec![0x11; 16],
			human_version: "1.0.0".to_string(),
			channel: "release".to_string(),
			kind: "mod".to_string(),
			declared_time: 1_760_000_000,
			compatibility: vec![Compatibility {
				game_version_predicate: game_predicate,
				loader_id: Some("fabric".to_string()),
				loader_version_predicate: loader_predicate,
				side: Side::Both,
				runtime_predicate: None,
				os_predicate: None,
				arch_predicate: None,
			}],
			artifacts: vec![Artifact {
				digest: vec![0xAB; 32],
				size: 10,
				media_type: "application/java-archive".to_string(),
				filename: "example.jar".to_string(),
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
		let signed = sign_payload(ObjectKind::Release, &release, &[&signer]);
		let digest = moraine_crypto::object_id(ObjectKind::Release, &signed.payload_bytes).to_vec();
		let wire = signed.wire_bytes();
		StoredObject {
			digest,
			kind: "release".to_string(),
			payload: signed.payload_bytes,
			wire,
		}
	}

	#[test]
	fn a_range_without_a_known_ordering_is_not_evaluated() {
		let object = release_object(Predicate::new(Scheme::Semver, vec![">=1.0.0".to_string()]), None);

		assert!(!release_matches_game_version(&object, "1.20.1", None));
		assert!(release_matches_game_version(&object, "1.20.1", Some(OrderingScheme::Semver)));
	}

	#[test]
	fn an_exact_match_needs_no_ordering() {
		let object = release_object(Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]), None);

		assert!(release_matches_game_version(&object, "1.20.1", None));
		assert!(!release_matches_game_version(&object, "1.19.0", None));
	}

	#[test]
	fn a_loader_without_a_version_predicate_matches_any_version() {
		let object = release_object(Predicate::new(Scheme::Any, Vec::new()), None);

		assert!(release_matches_loader_version(&object, "fabric", "0.15.0", None));
	}
}
