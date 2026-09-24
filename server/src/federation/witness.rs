use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use moraine_crypto::{ALG_ED25519, VerifyingKey};
use moraine_model::Canonical;
use moraine_model::witness::{WITNESS_DOMAIN, WitnessBundle, WitnessObservation};
use serde::Deserialize;

use super::storage_error;
use crate::auth::AuthenticatedUser;
use crate::registry;
use crate::routes::AppState;

pub(super) async fn observe(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Path(project_id): Path<String>,
) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let observations = match state.metadata.witness_observations(&project_id).await {
		Ok(observations) => observations,
		Err(error) => return storage_error(error),
	};
	let conflicts = registry::witness::witness_conflicts(&observations);
	let observations = observations
		.iter()
		.map(|row| {
			serde_json::json!({
				"observer_id": row.observer_id,
				"source_home": row.source_home,
				"sequence": row.sequence,
				"head_entry": row.head_entry,
				"observed_at": row.observed_at,
			})
		})
		.collect::<Vec<_>>();
	let conflicts = conflicts
		.iter()
		.map(|conflict| {
			serde_json::json!({
				"source_home": conflict.source_home,
				"sequence": conflict.sequence,
				"entries": conflict.entries,
				"observers": conflict.observers,
			})
		})
		.collect::<Vec<_>>();
	Json(serde_json::json!({
		"project_id": project_id,
		"observations": observations,
		"conflicts": conflicts,
	}))
	.into_response()
}

pub(super) async fn export(
	State(state): State<AppState>,
	user: AuthenticatedUser,
	Path(project_id): Path<String>,
) -> Response {
	if !user.allows("federation:manage") {
		return (StatusCode::FORBIDDEN, "the credential does not grant this scope").into_response();
	}
	let Some(signer) = &state.capability.webhook_signer else {
		return (StatusCode::SERVICE_UNAVAILABLE, "instance signing is unavailable").into_response();
	};
	let observations = match state.metadata.witness_observations(&project_id).await {
		Ok(observations) => observations
			.into_iter()
			.filter(|observation| observation.observer_id == "local")
			.filter_map(|observation| {
				Some(WitnessObservation {
					project_id: project_id.clone(),
					source_home: observation.source_home,
					sequence: u64::try_from(observation.sequence).ok()?,
					head_entry: observation.head_entry,
					observed_at: observation.observed_at,
				})
			})
			.collect::<Vec<_>>(),
		Err(error) => return storage_error(error),
	};
	let bundle = WitnessBundle {
		protocol: 1,
		observer_id: signer.key_id().to_string(),
		observations,
	};
	let payload = bundle.to_canonical_bytes();
	let mut message = WITNESS_DOMAIN.to_vec();
	message.extend_from_slice(&payload);
	Json(serde_json::json!({
		"payload": hex::encode(&payload),
		"signature": hex::encode(signer.sign(&message)),
		"public_key": hex::encode(signer.verifying_key().to_bytes()),
	}))
	.into_response()
}

#[derive(Deserialize)]
pub(super) struct WitnessEnvelope {
	payload: String,
	signature: String,
	public_key: String,
}

pub(super) async fn import(State(state): State<AppState>, Json(envelope): Json<WitnessEnvelope>) -> Response {
	let payload = match hex::decode(envelope.payload.trim()) {
		Ok(payload) => payload,
		Err(_) => return (StatusCode::BAD_REQUEST, "payload must be hex").into_response(),
	};
	let signature = match hex::decode(envelope.signature.trim()) {
		Ok(signature) => signature,
		Err(_) => return (StatusCode::BAD_REQUEST, "signature must be hex").into_response(),
	};
	let public_key = match hex::decode(envelope.public_key.trim()) {
		Ok(public_key) => public_key,
		Err(_) => return (StatusCode::BAD_REQUEST, "public_key must be hex").into_response(),
	};
	let key = match VerifyingKey::from_bytes(ALG_ED25519, &public_key) {
		Ok(key) => key,
		Err(_) => return (StatusCode::BAD_REQUEST, "public_key is invalid").into_response(),
	};
	let mut message = WITNESS_DOMAIN.to_vec();
	message.extend_from_slice(&payload);
	if key.verify(&message, &signature).is_err() {
		return (StatusCode::BAD_REQUEST, "witness signature did not verify").into_response();
	}
	let bundle = match WitnessBundle::from_canonical_bytes(&payload) {
		Ok(bundle) => bundle,
		Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
	};
	if bundle.observer_id != key.key_id().to_string() {
		return (StatusCode::BAD_REQUEST, "witness observer does not match its signing key").into_response();
	}
	let observer_id = bundle.observer_id.clone();
	let mut imported = 0;
	for observation in bundle.observations {
		if !valid_source_home(&observation.source_home) || observation.sequence > i64::MAX as u64 {
			return (StatusCode::BAD_REQUEST, "witness contains an invalid source home or sequence").into_response();
		}
		match state
			.metadata
			.record_witness_exchange(
				&observer_id,
				&observation.project_id,
				&observation.source_home,
				observation.sequence as i64,
				&observation.head_entry,
				observation.observed_at,
			)
			.await
		{
			Ok(()) => imported += 1,
			Err(error) => return storage_error(error),
		}
	}
	Json(serde_json::json!({ "observer_id": observer_id, "imported": imported })).into_response()
}

fn valid_source_home(value: &str) -> bool {
	let Ok(url) = reqwest::Url::parse(value) else { return false };
	if url.scheme() == "https" && url.host_str().is_some() {
		return true;
	}
	url.scheme() == "http" && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
}
