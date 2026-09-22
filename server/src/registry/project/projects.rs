use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use moraine_crypto::{ObjectKind, object_id};
use moraine_model::delegation::Delegation;
use moraine_model::signed::SignedObject;
use moraine_model::trust::verify_ownership_transfer;
use serde::Serialize;

use super::{bad_request, id_for, load_root, storage_error, stored};
use crate::db::StoredObject;
use crate::routes::AppState;
use crate::verify::{self, VerifyError};

#[derive(Serialize)]
struct ProjectReceipt {
	project_id: String,
	genesis: String,
}

#[derive(Serialize)]
struct ProjectSummary {
	project_id: String,
	genesis: String,
	head_seq: i64,
	head_entry: Option<String>,
	profile: Option<String>,
	owner: Option<OwnerView>,
	listing_state: moraine_model::search::ListingState,
	reason_code: Option<String>,
}

#[derive(Serialize)]
struct OwnerView {
	kind: String,
	id: String,
}

pub(super) async fn create_project(State(state): State<AppState>, body: Bytes) -> Response {
	let (_, object) = match verify::verify_genesis(&body) {
		Ok(verified) => verified,
		Err(error) => return bad_request(&state, error),
	};
	let digest = object.digest.to_vec();
	match state.metadata.project(&object.id).await {
		Ok(Some(existing)) if existing.genesis_digest != digest => {
			return (StatusCode::CONFLICT, "project id already bound to a different genesis").into_response();
		}
		Ok(Some(_)) => {}
		Ok(None) => {
			if state.capability.max_projects != 0 {
				let held = match state.metadata.table_count("projects").await {
					Ok(held) => held,
					Err(error) => return storage_error(error),
				};
				if held.max(0) as u64 >= state.capability.max_projects {
					return (
						StatusCode::FORBIDDEN,
						"this instance has reached its project limit".to_string(),
					)
						.into_response();
				}
			}
			if let Err(error) = state.metadata.put_object(&stored(&object)).await {
				return storage_error(error);
			}
			if let Err(error) = state.metadata.create_project(&object.id, &digest).await {
				return storage_error(error);
			}
		}
		Err(error) => return storage_error(error),
	}
	(
		StatusCode::CREATED,
		Json(ProjectReceipt {
			project_id: object.id,
			genesis: id_for(&digest),
		}),
	)
		.into_response()
}

pub(super) async fn project_summary(State(state): State<AppState>, Path(id): Path<String>) -> Response {
	match state.metadata.project(&id).await {
		Ok(Some(project)) => {
			let owner = match (project.owner_kind, project.owner_id) {
				(Some(kind), Some(id)) => Some(OwnerView { kind, id }),
				_ => None,
			};
			let policy = match state.metadata.listing_policy(&id).await {
				Ok(policy) => policy,
				Err(error) => return storage_error(error),
			};
			let listing_state = policy
				.as_ref()
				.map(|policy| policy.listing_state)
				.unwrap_or(moraine_model::search::ListingState::Listed);
			if listing_state == moraine_model::search::ListingState::Blocked {
				return (StatusCode::NOT_FOUND, "this instance does not serve that project").into_response();
			}
			let summary = ProjectSummary {
				project_id: project.id,
				genesis: id_for(&project.genesis_digest),
				head_seq: project.head_seq,
				head_entry: project.head_digest.as_deref().map(id_for),
				profile: project.profile_digest.as_deref().map(id_for),
				owner,
				listing_state,
				reason_code: policy.and_then(|policy| policy.reason_code),
			};
			Json(summary).into_response()
		}
		Ok(None) => (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => storage_error(error),
	}
}

#[derive(Serialize)]
struct TransferReceipt {
	transfer: String,
}

pub(super) async fn transfer(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
	match state.metadata.project(&id).await {
		Ok(Some(_)) => {}
		Ok(None) => return (StatusCode::NOT_FOUND, "no such project").into_response(),
		Err(error) => return storage_error(error),
	}
	let root = match load_root(&state, &id).await {
		Ok(root) => root,
		Err(response) => return *response,
	};
	let signed = match SignedObject::<Delegation>::from_bytes(&body) {
		Ok(signed) => signed,
		Err(error) => return bad_request(&state, VerifyError::Decode(error)),
	};
	if !matches!(signed.payload, Delegation::OwnershipTransfer(_)) {
		return (StatusCode::BAD_REQUEST, "object is not an ownership transfer").into_response();
	}
	if let Err(error) = verify_ownership_transfer(&signed, &root) {
		return bad_request(&state, VerifyError::Signature(error));
	}
	let digest = object_id(ObjectKind::Delegation, &signed.payload_bytes);
	let stored = StoredObject {
		digest: digest.to_vec(),
		kind: "delegation".to_string(),
		payload: signed.payload_bytes,
		wire: body.to_vec(),
	};
	if let Err(error) = state.metadata.put_object(&stored).await {
		return storage_error(error);
	}
	(
		StatusCode::CREATED,
		Json(TransferReceipt {
			transfer: id_for(&digest),
		}),
	)
		.into_response()
}

pub(super) async fn apply_withdrawal(state: &AppState, project_id: &str, object_digest: &[u8]) -> Result<(), Box<Response>> {
	let object = match state.metadata.object(object_digest).await {
		Ok(Some(object)) => object,
		Ok(None) => {
			return Err(Box::new(
				(StatusCode::CONFLICT, "withdrawal object is not stored").into_response(),
			));
		}
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	let signed = match SignedObject::<moraine_model::release::ReleaseObject>::from_bytes(&object.wire) {
		Ok(signed) => signed,
		Err(error) => return Err(Box::new(bad_request(state, VerifyError::Decode(error)))),
	};
	let moraine_model::release::ReleaseObject::Withdrawal(withdrawal) = signed.payload else {
		return Err(Box::new(
			(StatusCode::BAD_REQUEST, "object is not a withdrawal").into_response(),
		));
	};
	match state
		.metadata
		.record_withdrawal(
			project_id,
			&withdrawal.release_id,
			&withdrawal.reason,
			withdrawal.note.as_deref(),
			withdrawal.declared_time,
		)
		.await
	{
		Ok(()) => Ok(()),
		Err(error) => Err(Box::new(storage_error(error))),
	}
}

pub(super) async fn apply_ownership_transfer(
	state: &AppState,
	project_id: &str,
	object_digest: &[u8],
) -> Result<(), Box<Response>> {
	let project = match state.metadata.project(project_id).await {
		Ok(Some(project)) => project,
		Ok(None) => return Err(Box::new((StatusCode::NOT_FOUND, "no such project").into_response())),
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	let object = match state.metadata.object(object_digest).await {
		Ok(Some(object)) => object,
		Ok(None) => {
			return Err(Box::new(
				(StatusCode::CONFLICT, "transfer object is not stored").into_response(),
			));
		}
		Err(error) => return Err(Box::new(storage_error(error))),
	};
	let signed = match SignedObject::<Delegation>::from_bytes(&object.wire) {
		Ok(signed) => signed,
		Err(error) => return Err(Box::new(bad_request(state, VerifyError::Decode(error)))),
	};
	let Delegation::OwnershipTransfer(record) = &signed.payload else {
		return Err(Box::new(
			(StatusCode::BAD_REQUEST, "object is not an ownership transfer").into_response(),
		));
	};
	let root = load_root(state, project_id).await?;
	if let Err(error) = verify_ownership_transfer(&signed, &root) {
		return Err(Box::new(bad_request(state, VerifyError::Signature(error))));
	}
	if let (Some(current_kind), Some(current_id)) = (&project.owner_kind, &project.owner_id)
		&& (current_kind != &record.from_owner.kind || current_id != &record.from_owner.id)
	{
		return Err(Box::new(
			(StatusCode::CONFLICT, "the transfer does not start from the current owner").into_response(),
		));
	}
	match state
		.metadata
		.set_project_owner(project_id, &record.to_owner.kind, &record.to_owner.id)
		.await
	{
		Ok(_) => Ok(()),
		Err(error) => Err(Box::new(storage_error(error))),
	}
}
