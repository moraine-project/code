use std::time::Duration;

use super::now;
use super::validation::validate_url;
use crate::routes::AppState;

const MAX_ATTEMPTS: i64 = 8;
const BACKOFF_BASE_SECONDS: i64 = 30;

pub async fn deliver_pending(state: &AppState, limit: i64) -> Result<usize, String> {
	let due = state
		.metadata
		.due_deliveries(now(), limit)
		.await
		.map_err(|error| error.to_string())?;
	let mut delivered = 0;
	for delivery in due {
		let allow_local = state.capability.allow_insecure_federation_local;
		let target = match validate_url(&delivery.url, allow_local) {
			Ok(url) => url,
			Err(()) => {
				let _ = state
					.metadata
					.finish_delivery(&delivery.id, delivery.attempt + 1, "failed", now(), None)
					.await;
				continue;
			}
		};
		let (host, port) = match target.host_str().zip(target.port_or_known_default()) {
			Some(pair) => pair,
			None => {
				let _ = state
					.metadata
					.finish_delivery(&delivery.id, delivery.attempt + 1, "failed", now(), None)
					.await;
				continue;
			}
		};
		let addresses = match crate::federation::egress::resolve_public(host, port, allow_local).await {
			Ok(addresses) => addresses,
			Err(_) => {
				let _ = state
					.metadata
					.finish_delivery(&delivery.id, delivery.attempt + 1, "failed", now(), None)
					.await;
				continue;
			}
		};
		let client = match crate::federation::egress::pinned(
			crate::federation::egress::client_builder(&state.capability.tls_extra_roots),
			host,
			port,
			&addresses,
		)
		.timeout(Duration::from_secs(15))
		.redirect(reqwest::redirect::Policy::none())
		.build()
		{
			Ok(client) => client,
			Err(error) => {
				let _ = state
					.metadata
					.finish_delivery(&delivery.id, delivery.attempt + 1, "failed", now(), None)
					.await;
				tracing::warn!(%error, "webhook client was not built");
				continue;
			}
		};
		let result = client
			.post(target)
			.header(reqwest::header::CONTENT_TYPE, "application/json")
			.body(delivery.body)
			.send()
			.await;
		match result {
			Ok(response) if response.status().is_success() => {
				state
					.metadata
					.finish_delivery(&delivery.id, delivery.attempt + 1, "delivered", now(), Some(now()))
					.await
					.map_err(|error| error.to_string())?;
				delivered += 1;
			}
			_ => {
				let attempt = delivery.attempt + 1;
				if attempt >= MAX_ATTEMPTS {
					state
						.metadata
						.finish_delivery(&delivery.id, attempt, "failed", now(), None)
						.await
						.map_err(|error| error.to_string())?;
				} else {
					let backoff = BACKOFF_BASE_SECONDS.saturating_mul(1i64 << attempt.min(10));
					state
						.metadata
						.finish_delivery(&delivery.id, attempt, "pending", now() + backoff, None)
						.await
						.map_err(|error| error.to_string())?;
				}
			}
		}
	}
	Ok(delivered)
}
