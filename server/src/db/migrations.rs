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
