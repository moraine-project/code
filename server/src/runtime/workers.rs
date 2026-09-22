use crate::config::Config;
use crate::routes::AppState;

pub(super) fn spawn(state: AppState, config: Config) {
	spawn_webhooks(state.clone());
	spawn_scanner(state.clone(), config.clone());
	spawn_maintenance(state, config);
}

fn spawn_webhooks(state: AppState) {
	tokio::spawn(async move {
		let mut ticker = tokio::time::interval(std::time::Duration::from_secs(10));
		loop {
			ticker.tick().await;
			if let Err(error) = crate::federation::webhooks::deliver_pending(&state, 20).await {
				tracing::warn!(%error, "webhook dispatch failed");
			}
		}
	});
}

fn spawn_scanner(state: AppState, config: Config) {
	tokio::spawn(async move {
		crate::registry::scanner::run_worker(state, config).await;
	});
}

fn spawn_maintenance(state: AppState, config: Config) {
	tokio::spawn(async move {
		if config.maintenance_interval_seconds == 0 {
			return;
		}
		let mut ticker = tokio::time::interval(std::time::Duration::from_secs(config.maintenance_interval_seconds));
		loop {
			ticker.tick().await;
			maintenance_cycle(&state, &config).await;
		}
	});
}

async fn maintenance_cycle(state: &AppState, config: &Config) {
	let now = unix_now();
	let notifications_before = now - (moraine_model::event::NOTIFICATION_RETENTION_DAYS * 86_400) as i64;
	let deliveries_before = now - (moraine_model::event::WEBHOOK_RETENTION_DAYS * 86_400) as i64;
	if let Ok(pruned) = state.metadata.prune_notifications(notifications_before).await
		&& pruned > 0
	{
		tracing::info!(pruned, "pruned notifications");
	}
	if let Ok(pruned) = state.metadata.prune_deliveries(deliveries_before).await
		&& pruned > 0
	{
		tracing::info!(pruned, "pruned webhook deliveries");
	}
	if let Ok(synced) = crate::federation::resync_definitions(state).await
		&& synced > 0
	{
		tracing::info!(synced, "resynced definitions");
	}
	match crate::federation::resync_subscriptions(state).await {
		Ok(report) if report.synced > 0 || report.failed > 0 => {
			tracing::info!(synced = report.synced, failed = report.failed, "resynced subscriptions");
		}
		Ok(_) => {}
		Err(error) => tracing::warn!(%error, "subscription resync failed"),
	}
	match crate::federation::probe_mirrors(state, state.capability.max_mirror_probes_per_cycle as i64).await {
		Ok(confirmed) if confirmed > 0 => tracing::info!(confirmed, "confirmed mirror holdings"),
		Ok(_) => {}
		Err(error) => tracing::warn!(%error, "mirror probe failed"),
	}
	match crate::ops::gc::collect(state, config.staging_retention_seconds, config.blob_retention_seconds).await {
		Ok(collected) if collected.staging > 0 || collected.blobs > 0 => {
			tracing::info!(staging = collected.staging, blobs = collected.blobs, "collected storage");
		}
		Ok(_) => {}
		Err(error) => tracing::warn!(%error, "storage collection failed"),
	}
}

fn unix_now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|duration| duration.as_secs() as i64)
		.unwrap_or(0)
}
