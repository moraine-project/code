use super::*;

pub async fn run_worker(state: AppState, config: Config) {
	if !config.scanner_enabled {
		return;
	}
	let key = match load_or_create_key(&config.data_dir.join("scanner.key")) {
		Some(key) => key,
		None => {
			tracing::error!("scanner enabled but provider signing key could not be loaded");
			return;
		}
	};
	let (command, args) = local_command(&config);
	let provider = Provider {
		id: config.scanner_provider_id.clone(),
		kind: config.scanner_kind.clone(),
		command,
		args,
		public_key: key.verifying_key().to_bytes().to_vec(),
		enabled: true,
	};
	if let Err(error) = state.metadata.put_scanner_provider(&provider, now()).await {
		tracing::error!(%error, "could not register local scanner provider");
		return;
	}
	loop {
		if let Ok(items) = state.metadata.auto_scan_digests().await {
			for (provider_id, digest) in items {
				let _ = state.metadata.enqueue_scan(&provider_id, &digest, "policy", now()).await;
			}
		}
		poll_subscriptions(&state).await;
		if let Ok(Some(job)) = state.metadata.claim_scan_job(now()).await {
			execute_job(&state, &config, &key, job).await;
		}
		tokio::time::sleep(Duration::from_secs(5)).await;
	}
}

fn local_command(config: &Config) -> (String, Vec<String>) {
	(config.scanner_command.clone(), config.scanner_args.clone())
}

async fn poll_subscriptions(state: &AppState) {
	let rows = match sqlx::query(
		"SELECT id, provider_id, endpoint, interval_seconds, last_polled_at FROM scanner_subscriptions WHERE enabled = 1",
	)
	.fetch_all(&state.metadata.pool)
	.await
	{
		Ok(rows) => rows,
		Err(error) => {
			tracing::warn!(%error, "scanner subscription lookup failed");
			return;
		}
	};
	for row in rows {
		let id: String = row.get("id");
		let provider_id: String = row.get("provider_id");
		let endpoint: String = row.get("endpoint");
		let interval: i64 = row.get("interval_seconds");
		let last: Option<i64> = row.get("last_polled_at");
		if last.is_some_and(|last| now() - last < interval) {
			continue;
		}
		let result = async {
			let body = reqwest::Client::new()
				.get(&endpoint)
				.send()
				.await
				.map_err(|error| error.to_string())?
				.error_for_status()
				.map_err(|error| error.to_string())?
				.text()
				.await
				.map_err(|error| error.to_string())?;
			let value: serde_json::Value = serde_json::from_str(&body).map_err(|error| error.to_string())?;
			let records = value
				.get("records")
				.and_then(serde_json::Value::as_array)
				.or_else(|| value.as_array())
				.ok_or_else(|| "subscription response must be an array or {records: []}".to_string())?;
			let provider = state
				.metadata
				.scanner_provider(&provider_id)
				.await
				.map_err(|error| error.to_string())?
				.ok_or_else(|| "subscription provider is not registered".to_string())?;
			let trusted = TrustedKey::new(&provider.public_key).map_err(|error| error.to_string())?;
			let mut imported = 0;
			for record in records {
				let Some(hex_wire) = record.as_str() else {
					continue;
				};
				let wire = hex::decode(hex_wire).map_err(|_| "subscription record is not hex".to_string())?;
				let signed = SignedObject::<AttestationObject>::from_bytes(&wire).map_err(|error| error.to_string())?;
				if verify_envelope(
					&signed.envelope,
					&signed.signed_message(ObjectKind::Attestation),
					&[trusted],
					1,
				)
				.is_err()
				{
					continue;
				}
				let AttestationObject::Evidence(attestation) = &signed.payload else {
					continue;
				};
				if attestation.signer_id != provider_id {
					continue;
				}
				let digest = object_id(ObjectKind::Attestation, &signed.payload_bytes);
				state
					.metadata
					.put_object(&StoredObject {
						digest: digest.to_vec(),
						kind: "attestation".to_string(),
						payload: signed.payload_bytes.clone(),
						wire,
					})
					.await
					.map_err(|error| error.to_string())?;
				state
					.metadata
					.insert_attestation(attestation, &digest)
					.await
					.map_err(|error| error.to_string())?;
				imported += 1;
			}
			Ok::<usize, String>(imported)
		}
		.await;
		match result {
			Ok(imported) => {
				let _ = sqlx::query("UPDATE scanner_subscriptions SET last_polled_at = $2 WHERE id = $1")
					.bind(&id)
					.bind(now())
					.execute(&state.metadata.pool)
					.await;
				if imported > 0 {
					tracing::info!(provider = %provider_id, imported, "imported scanner attestations");
				}
			}
			Err(error) => tracing::warn!(provider = %provider_id, %error, "scanner subscription poll failed"),
		}
	}
}

async fn execute_job(state: &AppState, config: &Config, key: &SigningKey, job: Job) {
	let Some(provider) = state.metadata.scanner_provider(&job.provider_id).await.ok().flatten() else {
		let _ = state
			.metadata
			.finish_scan_job(&job.id, "failed", None, Some("scanner provider is not configured"), now())
			.await;
		return;
	};
	let Ok(digest): Result<[u8; 32], _> = job.artifact_digest.as_slice().try_into() else {
		finish_error(state, &job.id, "invalid artifact digest".to_string()).await;
		return;
	};
	let Some(mut stream) = state.store.read(&digest, None).await.ok().flatten() else {
		let _ = state
			.metadata
			.finish_scan_job(&job.id, "failed", None, Some("artifact not found"), now())
			.await;
		return;
	};
	let temp = config.data_dir.join(format!("scanner-{}.artifact", job.id));
	let mut file = match tokio::fs::File::create(&temp).await {
		Ok(file) => file,
		Err(error) => {
			finish_error(state, &job.id, error.to_string()).await;
			return;
		}
	};
	use futures_util::StreamExt;
	while let Some(chunk) = stream.next().await {
		match chunk {
			Ok(bytes) => {
				if file.write_all(&bytes).await.is_err() {
					finish_error(state, &job.id, "artifact write failed".to_string()).await;
					return;
				}
			}
			Err(error) => {
				finish_error(state, &job.id, error.to_string()).await;
				return;
			}
		}
	}
	drop(file);
	let output = tokio::time::timeout(
		Duration::from_secs(config.scanner_timeout_seconds),
		adapter_for(provider.clone()).scan(&temp),
	)
	.await;
	let _ = tokio::fs::remove_file(&temp).await;
	let (status, result, error) = match output {
		Err(_) => ("failed", None, Some("scanner timed out".to_string())),
		Ok(Err(error)) => ("failed", None, Some(error.to_string())),
		Ok(Ok(output)) => {
			let successful = matches!(output.verdict.as_str(), "clean" | "finding");
			(
				if successful { "succeeded" } else { "failed" },
				Some(serde_json::to_value(output).unwrap_or_else(|_| serde_json::json!({"verdict": "error"}))),
				if successful {
					None
				} else {
					Some("scanner returned an execution error".to_string())
				},
			)
		}
	};
	if let Some(result) = result.as_ref()
		&& status == "succeeded"
	{
		let _ = publish_result(state, key, &provider.id, &job.artifact_digest, result).await;
	}
	let result_text = result.as_ref().map(serde_json::Value::to_string);
	let _ = state
		.metadata
		.finish_scan_job(&job.id, status, result_text.as_deref(), error.as_deref(), now())
		.await;
}

async fn publish_result(
	state: &AppState,
	key: &SigningKey,
	provider_id: &str,
	digest: &[u8],
	result: &serde_json::Value,
) -> Result<(), String> {
	let attestation = Attestation {
		protocol: 1,
		artifact_digest: digest.to_vec(),
		subject_kind: "release".to_string(),
		subject_id: format!("sha256:{}", hex::encode(digest)),
		kind: AttestationKind::ScannerResult,
		media_type: "application/vnd.moraine.scanner-result+json".to_string(),
		body_digest: None,
		body_inline: Some(result.to_string().into_bytes()),
		signer_id: provider_id.to_string(),
		issued_at: now(),
	};
	let object = AttestationObject::Evidence(attestation.clone());
	let signed = sign_payload(ObjectKind::Attestation, &object, &[key]);
	let object_digest = object_id(ObjectKind::Attestation, &signed.payload_bytes);
	let wire = signed.wire_bytes();
	state
		.metadata
		.put_object(&StoredObject {
			digest: object_digest.to_vec(),
			kind: "attestation".to_string(),
			payload: signed.payload_bytes,
			wire,
		})
		.await
		.map_err(|error| error.to_string())?;
	state
		.metadata
		.insert_attestation(&attestation, &object_digest)
		.await
		.map_err(|error| error.to_string())
}

async fn finish_error(state: &AppState, id: &str, error: String) {
	let _ = state.metadata.finish_scan_job(id, "failed", None, Some(&error), now()).await;
}
