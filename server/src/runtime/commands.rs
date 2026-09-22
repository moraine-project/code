use crate::config::{Command, Config};
use crate::ops::{backup, bootstrap};

pub(super) async fn run(config: &Config, command: Command) -> Result<(), Box<dyn std::error::Error>> {
	match command {
		Command::Backup { out } => match backup::run(config, &out).await {
			Ok(summary) => {
				println!("database snapshot: {}", out.join("metadata.sqlite").display());
				println!("blob inventory:    {}", out.join("blobs.txt").display());
				println!(
					"projects: {}  blobs: {}  bytes: {}",
					summary.projects, summary.blobs, summary.bytes
				);
				println!("back up publisher root keys separately; they are not in this backup");
				Ok(())
			}
			Err(error) => Err(error.into()),
		},
		Command::Migrate => {
			let applied = crate::db::migrations::migrate_url(&config.database_url()).await?;
			println!("{}: {} migration(s) applied", config.database_label(), applied);
			Ok(())
		}
		Command::VerifyBackup { dir } => match backup::verify(&dir).await {
			Ok(verified) => {
				println!(
					"backup ok: projects: {}  blobs: {}  bytes: {}",
					verified.projects, verified.blobs, verified.bytes
				);
				Ok(())
			}
			Err(error) => Err(error.into()),
		},
		Command::Restore { dir, force } => match backup::restore(config, &dir, force).await {
			Ok(restored) => {
				println!(
					"restored into {}: projects: {}  blobs: {}  bytes: {}",
					config.data_dir.display(),
					restored.projects,
					restored.blobs,
					restored.bytes
				);
				println!("the webhook signing key is not in a backup; copy it separately or a new one is generated");
				Ok(())
			}
			Err(error) => Err(error.into()),
		},
		Command::Bootstrap { email } => match bootstrap::run(config, &email).await {
			Ok(created) => {
				println!("operator account: {}", email.trim().to_lowercase());
				println!("operator id:      {}", created.user_id);
				println!("operator password: {}", created.password);
				if let Some(key) = created.webhook_public_key {
					println!("webhook public key: {key}");
				}
				println!("store the password now; it is not shown again");
				Ok(())
			}
			Err(error) => Err(error.into()),
		},
	}
}
