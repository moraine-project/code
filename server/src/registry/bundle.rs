use std::io::Write;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use moraine_model::Canonical;
use moraine_model::delegation::Delegation;
use moraine_model::genesis::Genesis;
use moraine_model::signed::SignedObject;
use moraine_model::trust::{RootSet, verify_key_delegation};

use super::{id_for, parse_hex_digest, storage_error};
use crate::routes::AppState;

pub(crate) fn routes() -> Router<AppState> {
	Router::new().route("/v1/projects/{id}/releases/{hex}/bundle", get(release_bundle))
}

async fn release_bundle(State(state): State<AppState>, Path((id, hex_digest)): Path<(String, String)>) -> Response {
	let Some(digest) = parse_hex_digest(&hex_digest) else {
		return (StatusCode::BAD_REQUEST, "invalid digest").into_response();
	};
	let Some(object) = (match state.metadata.object(&digest).await {
		Ok(object) => object,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::NOT_FOUND, "no such release").into_response();
	};
	let release = match moraine_model::release::ReleaseObject::from_canonical_bytes(&object.payload) {
		Ok(moraine_model::release::ReleaseObject::Release(release)) => release,
		Ok(_) => return (StatusCode::NOT_FOUND, "object is not a release").into_response(),
		Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "stored release does not decode").into_response(),
	};
	if release.project_id != id {
		return (StatusCode::NOT_FOUND, "release does not belong to this project").into_response();
	}

	let Some(project) = (match state.metadata.project(&id).await {
		Ok(project) => project,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::NOT_FOUND, "no such project").into_response();
	};
	let Some(genesis_object) = (match state.metadata.object(&project.genesis_digest).await {
		Ok(object) => object,
		Err(error) => return storage_error(error),
	}) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "project genesis is missing").into_response();
	};
	let Ok(genesis) = Genesis::from_canonical_bytes(&genesis_object.payload) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "stored genesis does not decode").into_response();
	};
	let Ok(root) = RootSet::from_genesis(&genesis) else {
		return (StatusCode::INTERNAL_SERVER_ERROR, "stored genesis does not build a root set").into_response();
	};

	let entry = match state.metadata.feed_entry_for_object(&id, &digest).await {
		Ok(entry) => entry,
		Err(error) => return storage_error(error),
	};
	let delegations = match state.metadata.objects_of_kind("delegation", 500).await {
		Ok(objects) => objects,
		Err(error) => return storage_error(error),
	};

	let mut archive = Vec::new();
	match build_archive(
		&mut archive,
		&genesis_object.wire,
		&object.wire,
		entry.as_ref().map(|row| row.wire.as_slice()),
		&delegations,
		&id,
		&digest,
		&genesis,
		&root,
	) {
		Ok(()) => {}
		Err(error) => {
			tracing::error!(%error, "verification bundle was not built");
			return (StatusCode::INTERNAL_SERVER_ERROR, "could not build the bundle").into_response();
		}
	}
	let mut response = archive.into_response();
	response
		.headers_mut()
		.insert(header::CONTENT_TYPE, "application/zip".parse().expect("valid header"));
	response.headers_mut().insert(
		header::CONTENT_DISPOSITION,
		format!("attachment; filename=\"moraine-{hex_digest}.zip\"")
			.parse()
			.expect("valid header"),
	);
	response
}

#[allow(clippy::too_many_arguments)]
fn build_archive(
	archive: &mut Vec<u8>,
	genesis_wire: &[u8],
	release_wire: &[u8],
	entry_wire: Option<&[u8]>,
	delegations: &[crate::db::StoredObject],
	project_id: &str,
	release_digest: &[u8],
	genesis: &Genesis,
	root: &RootSet,
) -> Result<(), Box<dyn std::error::Error>> {
	let mut writer = zip::ZipWriter::new(std::io::Cursor::new(archive));
	let options = zip::write::SimpleFileOptions::default();
	writer.start_file("genesis.cbor", options)?;
	writer.write_all(genesis_wire)?;
	writer.start_file("release.cbor", options)?;
	writer.write_all(release_wire)?;
	if let Some(entry_wire) = entry_wire {
		writer.start_file("entry.cbor", options)?;
		writer.write_all(entry_wire)?;
	}
	let mut delegated = 0;
	for object in delegations {
		let Ok(signed) = SignedObject::<Delegation>::from_bytes(&object.wire) else {
			continue;
		};
		let Delegation::Key(key) = &signed.payload else {
			continue;
		};
		if key.project_id != project_id || verify_key_delegation(&signed, root).is_err() {
			continue;
		}
		writer.start_file(format!("delegations/{}.cbor", id_for(&object.digest)), options)?;
		writer.write_all(&object.wire)?;
		delegated += 1;
	}
	writer.start_file("verify.md", options)?;
	writer.write_all(instructions(project_id, release_digest, genesis, delegated).as_bytes())?;
	writer.finish()?;
	Ok(())
}

fn instructions(project_id: &str, release_digest: &[u8], genesis: &Genesis, delegated: i64) -> String {
	let mut text = String::new();
	text.push_str("# Verification bundle\n\n");
	text.push_str(&format!("Project: {project_id}\n\n"));
	text.push_str(&format!("Release: {}\n\n", id_for(release_digest)));
	text.push_str(&format!("Threshold: {}\n\n", genesis.threshold));
	text.push_str("Root keys:\n\n");
	for root in &genesis.roots {
		text.push_str(&format!("- {} {}\n", root.key_id, hex::encode(&root.public_key)));
	}
	text.push('\n');
	if delegated > 0 {
		text.push_str(&format!(
			"The release may have been signed by one of the {delegated} delegated keys in `delegations/`.\n\n"
		));
	}
	text.push_str("Check the bytes yourself, offline:\n\n");
	text.push_str("    moraine-verify object --kind genesis genesis.cbor\n");
	for root in &genesis.roots {
		text.push_str(&format!(
			"    moraine-verify object --kind release release.cbor --root {} --threshold {}\n",
			hex::encode(&root.public_key),
			genesis.threshold
		));
	}
	text.push_str("\nThen compare a file you downloaded to the release:\n\n");
	text.push_str("    moraine-verify artifact --release release.cbor --file <your file> --root <root from above>\n\n");
	text.push_str(
		"A signature says who published the bytes and that they match the digest. It does not say the file is safe, and it is not a review.\n",
	);
	text
}
