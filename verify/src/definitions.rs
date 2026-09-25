use moraine_codec::Value;
use moraine_crypto::ObjectKind;
use moraine_model::Canonical;
use moraine_model::compatibility::{Predicate, Scheme};
use moraine_model::definition::{
	Category, DeclaredBy, GameDef, LoaderAcceptance, LoaderDef, LoaderObject, LoaderRelease, Qualification, RuntimeDef, Tag,
	VersionSyntax,
};
use moraine_model::reference::ArtifactRef;
use moraine_model::signed::sign_payload;
use moraine_model::version::{OrderingScheme, VersionCatalog};

use crate::generate::{
	DECLARED_AT, Outcome, object_vector, predicate_vector, public_hex, raw_vector, sample_id, signer, trust,
};
use crate::vector::Vector;

fn pv(name: &str, ordering: &str, catalog: &[&str], scheme: &str, values: &[&str], version: &str, expected: &str) -> Vector {
	let ordering = OrderingScheme::parse(ordering).expect("known ordering scheme");
	let catalog = VersionCatalog::new(ordering, catalog.iter().map(|value| value.to_string()).collect());
	let predicate = Predicate {
		scheme: scheme.to_string(),
		values: values.iter().map(|value| value.to_string()).collect(),
	};
	predicate_vector(name, catalog, predicate, version, expected)
}

fn category(id: &str) -> Category {
	Category {
		id: id.to_string(),
		label: id.to_string(),
		parent: None,
	}
}

fn tag(id: &str) -> Tag {
	Tag {
		id: id.to_string(),
		label: id.to_string(),
	}
}

fn build_game_def(version_ordering: &str, categories: Vec<Category>) -> GameDef {
	GameDef {
		protocol: 1,
		game_id: sample_id("minecraft"),
		display_name: "Minecraft".to_string(),
		version_syntax: VersionSyntax {
			kind: "semver".to_string(),
			pattern: None,
		},
		version_ordering: version_ordering.to_string(),
		version_catalog: Vec::new(),
		loaders_allowed: true,
		loader_authorities: vec![sample_id("fabric-authority")],
		categories,
		tags: vec![tag("client"), tag("server")],
		metadata_extractor: Some("minecraft/fabric-json".to_string()),
		install_adapter: Some("minecraft/default".to_string()),
		declared_time: DECLARED_AT,
	}
}

fn build_runtime_def() -> RuntimeDef {
	RuntimeDef {
		protocol: 1,
		runtime_id: sample_id("java"),
		kind: "java".to_string(),
		display_name: "Java".to_string(),
		version_ordering: "semver".to_string(),
		version_catalog: Vec::new(),
		declared_time: DECLARED_AT,
	}
}

fn build_loader_def() -> LoaderDef {
	LoaderDef {
		protocol: 1,
		loader_id: sample_id("fabric"),
		game_id: sample_id("minecraft"),
		display_name: "Fabric".to_string(),
		version_ordering: "semver".to_string(),
		version_catalog: Vec::new(),
		game_versions: None,
		bootstrap: Some(ArtifactRef {
			digest: vec![0x55; 32],
			size: 4096,
			media_type: "application/java-archive".to_string(),
		}),
		accepted_artifacts: None,
		declared_time: DECLARED_AT,
	}
}

fn build_loader_release() -> LoaderRelease {
	LoaderRelease {
		protocol: 1,
		loader_id: sample_id("fabric"),
		version_id: "0.15.0".to_string(),
		game_version_predicate: Predicate::new(Scheme::Exact, vec!["1.20.1".to_string()]),
		runtime_id: Some(sample_id("java")),
		runtime_predicate: None,
		bootstrap: None,
		declared_time: DECLARED_AT,
	}
}

fn build_loader_acceptance() -> LoaderAcceptance {
	LoaderAcceptance {
		protocol: 1,
		accepting_loader_id: sample_id("cleanroom"),
		accepted_loader_id: sample_id("forge"),
		game_id: sample_id("minecraft"),
		game_version_predicate: Some(Predicate::new(Scheme::Exact, vec!["1.12.2".to_string()])),
		loader_version_predicate: None,
		accepted_version_predicate: None,
		qualification: Qualification::Most,
		declared_by: DeclaredBy {
			kind: "loader-authority".to_string(),
			id: sample_id("cleanroom-authority"),
		},
		evidence_digest: None,
		declared_time: DECLARED_AT,
	}
}

fn acceptance_value(qualification: &str) -> Value {
	Value::map([
		(Value::text("protocol"), Value::int(1)),
		(Value::text("type"), Value::text("mapping")),
		(Value::text("accepting_loader_id"), Value::text(sample_id("cleanroom"))),
		(Value::text("accepted_loader_id"), Value::text(sample_id("forge"))),
		(Value::text("game_id"), Value::text(sample_id("minecraft"))),
		(Value::text("qualification"), Value::text(qualification)),
		(
			Value::text("declared_by"),
			Value::map([
				(Value::text("kind"), Value::text("loader-authority")),
				(Value::text("id"), Value::text(sample_id("cleanroom-authority"))),
			]),
		),
		(Value::text("declared_time"), Value::int(DECLARED_AT)),
	])
}

pub(crate) fn vectors() -> Result<Vec<Vector>, String> {
	let k1 = signer(1);
	let k2 = signer(2);
	let mut vectors = Vec::with_capacity(64);

	vectors.push(pv(
		"predicate-semver-prefix",
		"semver",
		&[],
		"semver",
		&["1.20"],
		"1.20.4",
		"satisfied",
	));
	vectors.push(pv(
		"predicate-semver-prefix-miss",
		"semver",
		&[],
		"semver",
		&["1.20"],
		"1.21.0",
		"not-satisfied",
	));
	vectors.push(pv(
		"predicate-semver-range",
		"semver",
		&[],
		"semver",
		&[">=1.4.0", "<2.0.0"],
		"1.9.9",
		"satisfied",
	));
	vectors.push(pv(
		"predicate-semver-range-exceeds",
		"semver",
		&[],
		"semver",
		&[">=1.4.0", "<2.0.0"],
		"2.0.0",
		"not-satisfied",
	));
	vectors.push(pv(
		"predicate-semver-prerelease",
		"semver",
		&[],
		"semver",
		&["=1.0.0-alpha"],
		"1.0.0-alpha",
		"satisfied",
	));
	vectors.push(pv(
		"predicate-semver-prerelease-order",
		"semver",
		&[],
		"semver",
		&["=1.0.0-alpha"],
		"1.0.0",
		"not-satisfied",
	));
	vectors.push(pv(
		"predicate-exact-hit",
		"semver",
		&[],
		"exact",
		&["1.20.1"],
		"1.20.1",
		"satisfied",
	));
	vectors.push(pv(
		"predicate-exact-miss",
		"semver",
		&[],
		"exact",
		&["1.20.1"],
		"1.20.2",
		"not-satisfied",
	));
	vectors.push(pv("predicate-any", "semver", &[], "any", &[], "9.9.9", "satisfied"));
	vectors.push(pv(
		"predicate-unknown-scheme",
		"semver",
		&[],
		"maven",
		&["[1.0,2.0)"],
		"1.5",
		"unknown",
	));
	vectors.push(pv(
		"predicate-scheme-mismatch",
		"semver",
		&[],
		"ordered-list",
		&[],
		"1.5",
		"unknown",
	));

	let catalog = ["1.16", "1.17", "1.18", "1.19", "1.20", "1.20.4"];
	vectors.push(pv(
		"predicate-ordered-range-exclusive",
		"ordered-list",
		&catalog,
		"ordered-list",
		&["1.17..1.20.4"],
		"1.19",
		"satisfied",
	));
	vectors.push(pv(
		"predicate-ordered-range-excludes-end",
		"ordered-list",
		&catalog,
		"ordered-list",
		&["1.17..1.20.4"],
		"1.20.4",
		"not-satisfied",
	));
	vectors.push(pv(
		"predicate-ordered-range-inclusive",
		"ordered-list",
		&catalog,
		"ordered-list",
		&["1.17..=1.20.4"],
		"1.20.4",
		"satisfied",
	));
	vectors.push(pv(
		"predicate-ordered-unknown-version",
		"ordered-list",
		&catalog,
		"ordered-list",
		&["1.17..1.20.4"],
		"1.21",
		"unknown",
	));

	let calendar = ["2024-01", "2024-02", "2024-03"];
	vectors.push(pv(
		"predicate-calendar-range",
		"calendar",
		&calendar,
		"calendar",
		&["2024-01..=2024-03"],
		"2024-02",
		"satisfied",
	));
	vectors.push(pv(
		"predicate-calendar-outside",
		"calendar",
		&calendar,
		"calendar",
		&["2024-01..2024-02"],
		"2024-03",
		"not-satisfied",
	));

	let game = build_game_def("semver", vec![category("utility"), category("qol")]);
	let signed_game = sign_payload(ObjectKind::GameDef, &game, &[&k1]).map_err(|error| error.to_string())?;
	vectors.push(object_vector(
		"game-def-valid",
		"definitions",
		ObjectKind::GameDef,
		&signed_game,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let bad_ordering = build_game_def("lexicographic", vec![category("utility")]);
	vectors.push(raw_vector(
		"game-def-unknown-ordering",
		"definitions",
		ObjectKind::GameDef,
		bad_ordering.to_value(),
		&[&k1],
		Outcome {
			verdict: "reject",
			reason: Some("invalid-field-value"),
		},
		Some(trust(&[&k1], 1)),
	));

	let duplicate = build_game_def("semver", vec![category("utility"), category("utility")]);
	vectors.push(raw_vector(
		"game-def-duplicate-category",
		"definitions",
		ObjectKind::GameDef,
		duplicate.to_value(),
		&[&k1],
		Outcome {
			verdict: "reject",
			reason: Some("invalid-field-value"),
		},
		Some(trust(&[&k1], 1)),
	));

	let runtime = build_runtime_def();
	let signed_runtime = sign_payload(ObjectKind::RuntimeDef, &runtime, &[&k1]).map_err(|error| error.to_string())?;
	vectors.push(object_vector(
		"runtime-def-valid",
		"definitions",
		ObjectKind::RuntimeDef,
		&signed_runtime,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let mut catalog_game = build_game_def("ordered-list", vec![category("utility")]);
	catalog_game.version_catalog = vec!["1.0".to_string(), "1.1".to_string(), "1.2".to_string()];
	let signed_catalog_game = sign_payload(ObjectKind::GameDef, &catalog_game, &[&k1]).map_err(|error| error.to_string())?;
	vectors.push(object_vector(
		"game-def-version-catalog",
		"definitions",
		ObjectKind::GameDef,
		&signed_catalog_game,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let mut duplicate_catalog = build_game_def("ordered-list", vec![category("utility")]);
	duplicate_catalog.version_catalog = vec!["1.0".to_string(), "1.0".to_string()];
	vectors.push(raw_vector(
		"game-def-duplicate-version-catalog",
		"definitions",
		ObjectKind::GameDef,
		duplicate_catalog.to_value(),
		&[&k1],
		Outcome {
			verdict: "reject",
			reason: Some("invalid-field-value"),
		},
		Some(trust(&[&k1], 1)),
	));

	let mut empty_catalog = build_game_def("ordered-list", vec![category("utility")]);
	empty_catalog.version_catalog = vec![String::new()];
	vectors.push(raw_vector(
		"game-def-empty-version-catalog-entry",
		"definitions",
		ObjectKind::GameDef,
		empty_catalog.to_value(),
		&[&k1],
		Outcome {
			verdict: "reject",
			reason: Some("invalid-field-value"),
		},
		Some(trust(&[&k1], 1)),
	));

	let mut catalog_runtime = build_runtime_def();
	catalog_runtime.version_catalog = vec!["17".to_string(), "21".to_string()];
	let signed_catalog_runtime =
		sign_payload(ObjectKind::RuntimeDef, &catalog_runtime, &[&k1]).map_err(|error| error.to_string())?;
	vectors.push(object_vector(
		"runtime-def-version-catalog",
		"definitions",
		ObjectKind::RuntimeDef,
		&signed_catalog_runtime,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let loader = build_loader_def();
	let signed_loader =
		sign_payload(ObjectKind::LoaderDef, &LoaderObject::Definition(loader), &[&k1]).map_err(|error| error.to_string())?;
	vectors.push(object_vector(
		"loader-def-valid",
		"definitions",
		ObjectKind::LoaderDef,
		&signed_loader,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let mut catalog_loader = build_loader_def();
	catalog_loader.version_catalog = vec!["0.15.0".to_string(), "0.16.0".to_string()];
	catalog_loader.game_versions = Some(Predicate::new(Scheme::Semver, vec![">=1.20.1".to_string()]));
	let signed_catalog_loader = sign_payload(ObjectKind::LoaderDef, &LoaderObject::Definition(catalog_loader), &[&k1])
		.map_err(|error| error.to_string())?;
	vectors.push(object_vector(
		"loader-def-version-catalog-and-game-versions",
		"definitions",
		ObjectKind::LoaderDef,
		&signed_catalog_loader,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let mut duplicate_loader_catalog = build_loader_def();
	duplicate_loader_catalog.version_catalog = vec!["0.15.0".to_string(), "0.15.0".to_string()];
	vectors.push(raw_vector(
		"loader-def-duplicate-version-catalog",
		"definitions",
		ObjectKind::LoaderDef,
		LoaderObject::Definition(duplicate_loader_catalog).to_value(),
		&[&k1],
		Outcome {
			verdict: "reject",
			reason: Some("invalid-field-value"),
		},
		Some(trust(&[&k1], 1)),
	));

	let release = build_loader_release();
	let signed_release =
		sign_payload(ObjectKind::LoaderDef, &LoaderObject::Release(release), &[&k1]).map_err(|error| error.to_string())?;
	vectors.push(object_vector(
		"loader-release-valid",
		"definitions",
		ObjectKind::LoaderDef,
		&signed_release,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let acceptance = build_loader_acceptance();
	let signed_acceptance = sign_payload(ObjectKind::LoaderDef, &LoaderObject::Acceptance(acceptance), &[&k1])
		.map_err(|error| error.to_string())?;
	vectors.push(object_vector(
		"loader-acceptance-directional",
		"loader-acceptance",
		ObjectKind::LoaderDef,
		&signed_acceptance,
		"accept",
		None,
		Some(trust(&[&k1], 1)),
	));

	let invalid_qualification = acceptance_value("definitely-not-valid");
	vectors.push(raw_vector(
		"loader-acceptance-invalid-qualification",
		"loader-acceptance",
		ObjectKind::LoaderDef,
		invalid_qualification,
		&[&k1],
		Outcome {
			verdict: "reject",
			reason: Some("invalid-field-value"),
		},
		Some(trust(&[&k1], 1)),
	));

	let mut extra_field = acceptance_value("most");
	if let Value::Map(pairs) = &mut extra_field {
		pairs.push((Value::text("oops"), Value::int(1)));
	}
	vectors.push(raw_vector(
		"loader-acceptance-unknown-field",
		"loader-acceptance",
		ObjectKind::LoaderDef,
		extra_field,
		&[&k1],
		Outcome {
			verdict: "reject",
			reason: Some("unknown-field"),
		},
		Some(trust(&[&k1], 1)),
	));

	let missing_field = Value::map([
		(Value::text("protocol"), Value::int(1)),
		(Value::text("type"), Value::text("release")),
		(Value::text("loader_id"), Value::text(sample_id("fabric"))),
		(
			Value::text("game_version_predicate"),
			Value::map([
				(Value::text("scheme"), Value::text("exact")),
				(Value::text("values"), Value::array([Value::text("1.20.1")])),
			]),
		),
		(Value::text("declared_time"), Value::int(DECLARED_AT)),
	]);
	vectors.push(raw_vector(
		"loader-release-missing-version",
		"definitions",
		ObjectKind::LoaderDef,
		missing_field,
		&[&k1],
		Outcome {
			verdict: "reject",
			reason: Some("missing-field"),
		},
		Some(trust(&[&k1], 1)),
	));

	let signed_by_delegated = sign_payload(ObjectKind::GameDef, &game, &[&k2]).map_err(|error| error.to_string())?;
	let mut delegated_trust = trust(&[&k1], 1);
	delegated_trust.delegated_keys = vec![public_hex(&k2)];
	vectors.push(object_vector(
		"game-def-bad-signature",
		"definitions",
		ObjectKind::GameDef,
		&signed_by_delegated,
		"reject",
		Some("bad-signature"),
		Some(delegated_trust),
	));

	Ok(vectors)
}
