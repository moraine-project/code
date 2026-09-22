use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use moraine_crypto::{ObjectKind, object_id};
use moraine_model::delegation::Delegation;
use moraine_model::signed::SignedObject;
use moraine_model::trust::{RootSet, verify_migration};
use serde::Serialize;

use super::{bad_request, load_delegations, load_root, storage_error, store_object_record, unix_now};
use crate::routes::AppState;
use crate::verify::{self, VerifyError};

#[derive(Serialize)]
struct ObjectReceipt {
	id: String,
	kind: String,
}

pub(super) async fn store_object(
	State(state): State<AppState>,
	Path((id, kind)): Path<(String, String)>,
	body: Bytes,
) -> Response {
	let Some(kind) = ObjectKind::parse(&kind) else {
		return (StatusCode::BAD_REQUEST, "unknown object kind").into_response();
	};
	if matches!(kind, ObjectKind::Genesis | ObjectKind::FeedEntry) {
		return (StatusCode::BAD_REQUEST, "genesis and feed entries use their own endpoints").into_response();
	}
	let root = match load_root(&state, &id).await {
		Ok(root) => root,
		Err(response) => return *response,
	};
	let delegations = match load_delegations(&state, &id, &root).await {
		Ok(delegations) => delegations,
		Err(response) => return *response,
	};
	let object = match migration_object(kind, &body, &root) {
		Some(result) => match result {
			Ok(object) => object,
			Err(error) => return bad_request(&state, error),
		},
		None => match verify::verify_object_authorized(kind, &body, &root, &delegations, unix_now()) {
			Ok(object) => object,
			Err(error) => return bad_request(&state, error),
		},
	};
	if let Err(error) = store_object_record(&state, &object).await {
		if let sqlx::Error::Protocol(message) = &error {
			return (StatusCode::BAD_REQUEST, message.clone()).into_response();
		}
		return storage_error(error);
	}
	let receipt = ObjectReceipt {
		id: object.id,
		kind: kind.as_str().to_string(),
	};
	(StatusCode::CREATED, Json(receipt)).into_response()
}

pub(super) fn migration_object(
	kind: ObjectKind,
	body: &[u8],
	root: &RootSet,
) -> Option<Result<verify::VerifiedObject, VerifyError>> {
	if kind != ObjectKind::Delegation {
		return None;
	}
	let signed = SignedObject::<Delegation>::from_bytes(body).ok()?;
	if !matches!(signed.payload, Delegation::Migration(_)) {
		return None;
	}
	Some(match verify_migration(&signed, root) {
		Ok(()) => Ok(verify::VerifiedObject {
			kind,
			digest: object_id(kind, &signed.payload_bytes),
			id: signed.id(kind),
			payload_bytes: signed.payload_bytes.clone(),
			wire_bytes: body.to_vec(),
		}),
		Err(error) => Err(VerifyError::Decode(error)),
	})
}
