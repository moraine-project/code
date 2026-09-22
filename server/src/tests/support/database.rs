use crate::db::MetadataStore;

static DATABASES: std::sync::Mutex<Vec<(std::path::PathBuf, String)>> = std::sync::Mutex::new(Vec::new());

pub(crate) async fn isolated_store() -> Option<MetadataStore> {
	let base = std::env::var("MORAINE_TEST_POSTGRES").ok()?;
	let schema = unique_schema();
	create_schema(&base, &schema).await;
	let separator = if base.contains('?') { '&' } else { '?' };
	let url = format!("{base}{separator}options=-csearch_path%3D{schema}");
	Some(MetadataStore::open_url(&url).await.expect("store"))
}

pub(crate) async fn store_for(directory: &std::path::Path) -> MetadataStore {
	let url = DATABASES
		.lock()
		.expect("databases")
		.iter()
		.find(|(path, _)| path == directory)
		.map(|(_, url)| url.clone())
		.unwrap_or_else(|| crate::db::sqlite_url(&directory.join("metadata.sqlite")));
	MetadataStore::open_url(&url).await.expect("store")
}

pub(super) async fn test_database(directory: &std::path::Path) -> String {
	let Ok(base) = std::env::var("MORAINE_TEST_POSTGRES") else {
		return crate::db::sqlite_url(&directory.join("metadata.sqlite"));
	};
	let schema = unique_schema();
	create_schema(&base, &schema).await;
	let separator = if base.contains('?') { '&' } else { '?' };
	let url = format!("{base}{separator}options=-csearch_path%3D{schema}");
	DATABASES
		.lock()
		.expect("databases")
		.push((directory.to_path_buf(), url.clone()));
	url
}

async fn create_schema(base: &str, schema: &str) {
	use sqlx::any::{AnyPoolOptions, install_default_drivers};
	install_default_drivers();
	let pool = AnyPoolOptions::new()
		.max_connections(1)
		.connect(base)
		.await
		.expect("admin pool");
	sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA IF NOT EXISTS {schema}")))
		.execute(&pool)
		.await
		.expect("create schema");
}

fn unique_schema() -> String {
	static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
	let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
	let nanos = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|elapsed| elapsed.as_nanos())
		.unwrap_or(0);
	format!("t{nanos:x}_{count:x}")
}

const TEST_OPERATORS: &[&str] = &[
	"ops@example.org",
	"moderator@example.org",
	"legal@example.org",
	"reviewer@example.org",
	"provider@example.org",
	"operator@example.org",
];

pub(crate) async fn seed_operators(metadata: &MetadataStore) {
	for (index, email) in TEST_OPERATORS.iter().enumerate() {
		if metadata.user_by_email(email).await.ok().flatten().is_some() {
			continue;
		}
		let hash = crate::auth::password::hash_password("correct horse battery").expect("hash");
		metadata
			.create_user(
				&format!("operator-{index}"),
				email,
				&hash,
				crate::auth::OPERATOR_ROLE,
				1_760_000_000,
			)
			.await
			.expect("seed operator");
	}
}
