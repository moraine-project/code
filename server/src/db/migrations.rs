use std::path::Path;

use sqlx::AnyPool;

use super::{Engine, connect};

const MIGRATION_LOCK_KEY: i64 = 0x6d6f7261696e65;

struct Migration {
	name: &'static str,
	sqlite: &'static str,
	postgres: &'static str,
}

impl Migration {
	fn sql(&self, engine: Engine) -> &'static str {
		match engine {
			Engine::Sqlite => self.sqlite,
			Engine::Postgres => self.postgres,
		}
	}
}

const MIGRATIONS: &[Migration] = &[
	Migration {
		name: "0001_initial",
		sqlite: include_str!("../../migrations/sqlite/0001_initial.sql"),
		postgres: include_str!("../../migrations/postgres/0001_initial.sql"),
	},
	Migration {
		name: "0002_search_description",
		sqlite: include_str!("../../migrations/sqlite/0002_search_description.sql"),
		postgres: include_str!("../../migrations/postgres/0002_search_description.sql"),
	},
	Migration {
		name: "0003_mirror_confirmations",
		sqlite: include_str!("../../migrations/sqlite/0003_mirror_confirmations.sql"),
		postgres: include_str!("../../migrations/postgres/0003_mirror_confirmations.sql"),
	},
	Migration {
		name: "0004_blob_uploads",
		sqlite: include_str!("../../migrations/sqlite/0004_blob_uploads.sql"),
		postgres: include_str!("../../migrations/postgres/0004_blob_uploads.sql"),
	},
	Migration {
		name: "0005_search_changelog",
		sqlite: include_str!("../../migrations/sqlite/0005_search_changelog.sql"),
		postgres: include_str!("../../migrations/postgres/0005_search_changelog.sql"),
	},
	Migration {
		name: "0006_loader_releases",
		sqlite: include_str!("../../migrations/sqlite/0006_loader_releases.sql"),
		postgres: include_str!("../../migrations/postgres/0006_loader_releases.sql"),
	},
	Migration {
		name: "0007_directory_policy",
		sqlite: include_str!("../../migrations/sqlite/0007_directory_policy.sql"),
		postgres: include_str!("../../migrations/postgres/0007_directory_policy.sql"),
	},
	Migration {
		name: "0008_legal_requests",
		sqlite: include_str!("../../migrations/sqlite/0008_legal_requests.sql"),
		postgres: include_str!("../../migrations/postgres/0008_legal_requests.sql"),
	},
	Migration {
		name: "0009_loader_accepts",
		sqlite: include_str!("../../migrations/sqlite/0009_loader_accepts.sql"),
		postgres: include_str!("../../migrations/postgres/0009_loader_accepts.sql"),
	},
	Migration {
		name: "0010_evidence_attestations",
		sqlite: include_str!("../../migrations/sqlite/0010_evidence_attestations.sql"),
		postgres: include_str!("../../migrations/postgres/0010_evidence_attestations.sql"),
	},
	Migration {
		name: "0011_impersonation_reports",
		sqlite: include_str!("../../migrations/sqlite/0011_impersonation_reports.sql"),
		postgres: include_str!("../../migrations/postgres/0011_impersonation_reports.sql"),
	},
	Migration {
		name: "0012_sanctions",
		sqlite: include_str!("../../migrations/sqlite/0012_sanctions.sql"),
		postgres: include_str!("../../migrations/postgres/0012_sanctions.sql"),
	},
	Migration {
		name: "0013_search_trigrams",
		sqlite: include_str!("../../migrations/sqlite/0013_search_trigrams.sql"),
		postgres: include_str!("../../migrations/postgres/0013_search_trigrams.sql"),
	},
	Migration {
		name: "0014_recovery",
		sqlite: include_str!("../../migrations/sqlite/0014_recovery.sql"),
		postgres: include_str!("../../migrations/postgres/0014_recovery.sql"),
	},
	Migration {
		name: "0015_project_migrations",
		sqlite: include_str!("../../migrations/sqlite/0015_project_migrations.sql"),
		postgres: include_str!("../../migrations/postgres/0015_project_migrations.sql"),
	},
	Migration {
		name: "0016_deny_lists",
		sqlite: include_str!("../../migrations/sqlite/0016_deny_lists.sql"),
		postgres: include_str!("../../migrations/postgres/0016_deny_lists.sql"),
	},
	Migration {
		name: "0017_witness_observations",
		sqlite: include_str!("../../migrations/sqlite/0017_witness_observations.sql"),
		postgres: include_str!("../../migrations/postgres/0017_witness_observations.sql"),
	},
	Migration {
		name: "0018_definition_source",
		sqlite: include_str!("../../migrations/sqlite/0018_definition_source.sql"),
		postgres: include_str!("../../migrations/postgres/0018_definition_source.sql"),
	},
	Migration {
		name: "0019_instance_role",
		sqlite: include_str!("../../migrations/sqlite/0019_instance_role.sql"),
		postgres: include_str!("../../migrations/postgres/0019_instance_role.sql"),
	},
	Migration {
		name: "0020_project_delegations",
		sqlite: include_str!("../../migrations/sqlite/0020_project_delegations.sql"),
		postgres: include_str!("../../migrations/postgres/0020_project_delegations.sql"),
	},
	Migration {
		name: "0021_recovery_codes",
		sqlite: include_str!("../../migrations/sqlite/0021_recovery_codes.sql"),
		postgres: include_str!("../../migrations/postgres/0021_recovery_codes.sql"),
	},
	Migration {
		name: "0022_email_verification",
		sqlite: include_str!("../../migrations/sqlite/0022_email_verification.sql"),
		postgres: include_str!("../../migrations/postgres/0022_email_verification.sql"),
	},
	Migration {
		name: "0023_publication_grants",
		sqlite: include_str!("../../migrations/sqlite/0023_publication_grants.sql"),
		postgres: include_str!("../../migrations/postgres/0023_publication_grants.sql"),
	},
	Migration {
		name: "0024_external_projects",
		sqlite: include_str!("../../migrations/sqlite/0024_external_projects.sql"),
		postgres: include_str!("../../migrations/postgres/0024_external_projects.sql"),
	},
	Migration {
		name: "0025_witness_exchange",
		sqlite: include_str!("../../migrations/sqlite/0025_witness_exchange.sql"),
		postgres: include_str!("../../migrations/postgres/0025_witness_exchange.sql"),
	},
	Migration {
		name: "0026_scanners",
		sqlite: include_str!("../../migrations/sqlite/0026_scanners.sql"),
		postgres: include_str!("../../migrations/postgres/0026_scanners.sql"),
	},
	Migration {
		name: "0027_scanner_local_provider",
		sqlite: include_str!("../../migrations/sqlite/0027_scanner_local_provider.sql"),
		postgres: include_str!("../../migrations/postgres/0027_scanner_local_provider.sql"),
	},
	Migration {
		name: "0028_scanner_integer_flags",
		sqlite: include_str!("../../migrations/sqlite/0028_scanner_integer_flags.sql"),
		postgres: include_str!("../../migrations/postgres/0028_scanner_integer_flags.sql"),
	},
	Migration {
		name: "0029_project_owner_keys",
		sqlite: include_str!("../../migrations/sqlite/0029_project_owner_keys.sql"),
		postgres: include_str!("../../migrations/postgres/0029_project_owner_keys.sql"),
	},
];

pub(super) async fn run_migrations(pool: &AnyPool, engine: Engine) -> Result<usize, sqlx::Error> {
	let mut connection = pool.acquire().await?;
	sqlx::query("CREATE TABLE IF NOT EXISTS schema_migrations (name TEXT PRIMARY KEY, applied_at BIGINT NOT NULL)")
		.execute(&mut *connection)
		.await?;
	let mut applied = 0;
	for migration in MIGRATIONS {
		let begin = match engine {
			Engine::Sqlite => "BEGIN IMMEDIATE",
			Engine::Postgres => "BEGIN",
		};
		sqlx::query(begin).execute(&mut *connection).await?;
		if engine == Engine::Postgres {
			sqlx::query("SELECT pg_advisory_xact_lock($1)")
				.bind(MIGRATION_LOCK_KEY)
				.execute(&mut *connection)
				.await?;
		}
		let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM schema_migrations WHERE name = $1")
			.bind(migration.name)
			.fetch_one(&mut *connection)
			.await?;
		if exists > 0 {
			sqlx::query("ROLLBACK").execute(&mut *connection).await?;
			continue;
		}
		sqlx::raw_sql(migration.sql(engine)).execute(&mut *connection).await?;
		sqlx::query("INSERT INTO schema_migrations (name, applied_at) VALUES ($1, $2) ON CONFLICT(name) DO NOTHING")
			.bind(migration.name)
			.bind(unix_now())
			.execute(&mut *connection)
			.await?;
		sqlx::query("COMMIT").execute(&mut *connection).await?;
		applied += 1;
	}
	Ok(applied)
}

pub async fn migrate_url(url: &str) -> Result<usize, sqlx::Error> {
	if let Some(parent) = super::sqlite_path(url).and_then(Path::parent) {
		std::fs::create_dir_all(parent).map_err(sqlx::Error::Io)?;
	}
	let pool = connect(url, 1).await?;
	run_migrations(&pool, Engine::of(url)).await
}

pub async fn pending_url(url: &str) -> Result<usize, sqlx::Error> {
	let engine = Engine::of(url);
	if let Some(path) = super::sqlite_path(url)
		&& !path.exists()
	{
		return Ok(MIGRATIONS.len());
	}
	let pool = connect(url, 1).await?;
	let tracked = match engine {
		Engine::Sqlite => {
			sqlx::query_scalar::<_, i64>(
				"SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
			)
			.fetch_one(&pool)
			.await?
		}
		Engine::Postgres => {
			sqlx::query_scalar::<_, i64>(
				"SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'schema_migrations'",
			)
			.fetch_one(&pool)
			.await?
		}
	};
	if tracked == 0 {
		return Ok(MIGRATIONS.len());
	}
	let applied = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM schema_migrations")
		.fetch_one(&pool)
		.await?;
	Ok(MIGRATIONS.len().saturating_sub(applied as usize))
}

fn unix_now() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|elapsed| elapsed.as_secs() as i64)
		.unwrap_or(0)
}

#[cfg(test)]
mod migration_tests {
	use super::*;
	use crate::db::{MetadataStore, sqlite_url};

	#[test]
	fn postgres_migrations_do_not_use_sqlite_types() {
		for migration in MIGRATIONS {
			let sql = migration.postgres.to_uppercase();
			assert!(
				!sql.contains(" BLOB ") && !sql.contains(" BLOB,") && !sql.contains(" BLOB)"),
				"{} uses a SQLite-only type in its Postgres migration",
				migration.name
			);
		}
	}

	#[tokio::test]
	async fn migrations_are_pending_until_applied() {
		let directory = tempfile::tempdir().expect("tempdir");
		let url = sqlite_url(&directory.path().join("metadata.sqlite"));

		assert_eq!(pending_url(&url).await.expect("pending"), MIGRATIONS.len());

		assert_eq!(migrate_url(&url).await.expect("migrate"), MIGRATIONS.len());
		assert_eq!(pending_url(&url).await.expect("pending"), 0);
		assert_eq!(migrate_url(&url).await.expect("migrate again"), 0);
	}

	#[tokio::test]
	async fn concurrent_openers_apply_migrations_once() {
		let directory = tempfile::tempdir().expect("tempdir");
		let path = directory.path().join("metadata.sqlite");

		let (first, second) = tokio::join!(MetadataStore::open(&path), MetadataStore::open(&path));

		assert!(first.is_ok(), "{:?}", first.err());
		assert!(second.is_ok(), "{:?}", second.err());
		assert_eq!(pending_url(&sqlite_url(&path)).await.expect("pending"), 0);
	}
}
