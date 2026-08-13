use crate::config::Config;

pub struct Bootstrapped {
	pub user_id: String,
	pub password: String,
	pub webhook_public_key: Option<String>,
}

pub async fn run(config: &Config, email: &str) -> Result<Bootstrapped, String> {
	let email = email.trim().to_lowercase();
	if email.len() < 3 || !email.contains('@') {
		return Err("the operator email is not a valid address".to_string());
	}
	std::fs::create_dir_all(&config.data_dir).map_err(|error| error.to_string())?;
	let metadata = crate::store::MetadataStore::open(config.data_dir.join("metadata.sqlite"))
		.await
		.map_err(|error| error.to_string())?;
	if metadata
		.user_by_email(&email)
		.await
		.map_err(|error| error.to_string())?
		.is_some()
	{
		return Err(format!("an account for {email} already exists"));
	}
	let password = random_password();
	let hash = crate::password::hash_password(&password).map_err(|_| "could not hash the password".to_string())?;
	let user_id = random_id();
	metadata
		.create_user(&user_id, &email, &hash, unix_now())
		.await
		.map_err(|error| error.to_string())?;
	let capability = crate::capability::Capability::discover(config);
	Ok(Bootstrapped {
		user_id,
		password,
		webhook_public_key: capability.webhook_public_key,
	})
}

fn random_password() -> String {
	format!("{}{}", random_id(), random_id())
}

fn random_id() -> String {
	let mut bytes = [0u8; 16];
	if getrandom::fill(&mut bytes).is_err() {
		panic!("operating system randomness is unavailable");
	}
	hex::encode(bytes)
}

fn unix_now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|elapsed| elapsed.as_secs() as i64)
		.unwrap_or(0)
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use super::*;
	use crate::store::MetadataStore;

	async fn config(directory: &std::path::Path) -> Config {
		Config {
			bind: "127.0.0.1:0".parse().expect("addr"),
			data_dir: directory.to_path_buf(),
			max_artifact_bytes: 1024,
			max_feed_page_entries: 100,
			max_response_bytes: 16_777_216,
			staging_retention_seconds: 3_600,
			blob_retention_seconds: 604_800,
			max_sync_pages: 200,
			requests_per_minute: 600,
			max_concurrent_syncs: 4,
			tls_extra_roots: None,
			max_feed_scan_pages: 50,
			skip_migrate_on_start: false,
			publishing: crate::config::Publishing::Review,
			allow_insecure_federation_local: false,
			web_dir: None,
		}
	}

	#[tokio::test]
	async fn creates_an_operator_account_and_signing_key_once() {
		let directory = tempfile::tempdir().expect("tempdir");
		let config = config(directory.path()).await;

		let first = run(&config, "ops@example.org").await.expect("bootstrap");

		assert!(first.password.len() >= 12);
		assert!(first.webhook_public_key.is_some());
		assert!(directory.path().join("webhook.key").exists());
		let metadata = Arc::new(
			MetadataStore::open(directory.path().join("metadata.sqlite"))
				.await
				.expect("metadata"),
		);
		let user = metadata
			.user_by_email("ops@example.org")
			.await
			.expect("lookup")
			.expect("account");
		assert!(crate::password::verify_password(&first.password, &user.password_hash));

		let again = run(&config, "ops@example.org").await;
		assert!(again.is_err());
	}
}
