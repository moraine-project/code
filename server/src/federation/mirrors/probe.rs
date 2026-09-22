use sha2::Digest;

use super::now;
use super::store::DueCommitmentRow;
use crate::routes::AppState;

pub async fn probe_mirrors(state: &AppState, limit: i64) -> Result<usize, String> {
	let checked_before = now() - PROBE_INTERVAL_SECONDS;
	let due = state
		.metadata
		.commitments_due(checked_before, limit, state.capability.max_mirror_probe_bytes)
		.await
		.map_err(|error| error.to_string())?;
	let mut confirmed = 0;
	for commitment in due {
		let reachable = confirm(&state.capability, &commitment).await;
		if reachable {
			confirmed += 1;
		}
		state
			.metadata
			.record_confirmation(&commitment.artifact_digest, &commitment.mirror_id, now(), reachable)
			.await
			.map_err(|error| error.to_string())?;
	}
	Ok(confirmed)
}

const PROBE_INTERVAL_SECONDS: i64 = 86_400;
const PROBE_TIMEOUT_SECONDS: u64 = 30;

async fn confirm(capability: &crate::capability::Capability, commitment: &DueCommitmentRow) -> bool {
	let Ok(mut base) = reqwest::Url::parse(&commitment.endpoint) else {
		return false;
	};
	if !matches!(base.scheme(), "https" | "http") {
		return false;
	}
	let insecure_http = base.scheme() == "http";
	if insecure_http && !capability.allow_insecure_federation_local {
		return false;
	}
	let Some(host) = base.host_str().map(str::to_string) else {
		return false;
	};
	let Some(port) = base.port_or_known_default() else {
		return false;
	};
	let Ok(addresses) =
		crate::federation::egress::resolve_public(&host, port, capability.allow_insecure_federation_local).await
	else {
		return false;
	};
	if insecure_http && !addresses.iter().all(std::net::IpAddr::is_loopback) {
		return false;
	}
	let Ok(client) = crate::federation::egress::pinned(
		crate::federation::egress::client_builder(&capability.tls_extra_roots),
		&host,
		port,
		&addresses,
	)
	.timeout(std::time::Duration::from_secs(PROBE_TIMEOUT_SECONDS))
	.redirect(reqwest::redirect::Policy::none())
	.build() else {
		return false;
	};
	let path = format!("/v1/blobs/sha256/{}", hex::encode(&commitment.artifact_digest));
	base.set_path(&path);
	let Ok(mut response) = client.get(base).send().await else {
		return false;
	};
	if !response.status().is_success() {
		return false;
	}
	if let Some(length) = response.content_length()
		&& length != commitment.size as u64
	{
		return false;
	}
	let mut hasher = sha2::Sha256::new();
	let mut received = 0u64;
	loop {
		match response.chunk().await {
			Ok(Some(chunk)) => {
				received += chunk.len() as u64;
				if received > commitment.size as u64 {
					return false;
				}
				hasher.update(&chunk);
			}
			Ok(None) => break,
			Err(_) => return false,
		}
	}
	received == commitment.size as u64 && hasher.finalize().as_slice() == commitment.artifact_digest.as_slice()
}
