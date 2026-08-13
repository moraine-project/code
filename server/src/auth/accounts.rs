use sqlx::Row;

use crate::db::MetadataStore;

#[derive(Debug, Clone)]
pub struct UserRow {
	pub id: String,
	pub email: String,
	pub password_hash: String,
}

#[derive(Debug, Clone)]
pub struct SessionRow {
	pub id: String,
	pub user_id: String,
	pub token_hash: Vec<u8>,
	pub created_at: i64,
	pub idle_expires_at: i64,
	pub absolute_expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct ApiKeyRow {
	pub id: String,
	pub user_id: String,
	pub name: String,
	pub prefix: String,
	pub secret_hash: Vec<u8>,
	pub scopes: String,
	pub created_at: i64,
	pub expires_at: Option<i64>,
	pub last_used_at: Option<i64>,
}

impl MetadataStore {
	pub async fn create_user(&self, id: &str, email: &str, password_hash: &str, created_at: i64) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		sqlx::query("INSERT INTO users (id, email, created_at) VALUES ($1, $2, $3)")
			.bind(id)
			.bind(email)
			.bind(created_at)
			.execute(&mut *transaction)
			.await?;
		sqlx::query("INSERT INTO user_credentials (user_id, secret_hash, updated_at) VALUES ($1, $2, $3)")
			.bind(id)
			.bind(password_hash)
			.bind(created_at)
			.execute(&mut *transaction)
			.await?;
		transaction.commit().await?;
		Ok(())
	}

	pub async fn user_by_email(&self, email: &str) -> Result<Option<UserRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT u.id, u.email, c.secret_hash
			 FROM users u JOIN user_credentials c ON c.user_id = u.id WHERE u.email = $1",
		)
		.bind(email)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| UserRow {
			id: row.get("id"),
			email: row.get("email"),
			password_hash: row.get("secret_hash"),
		}))
	}

	pub async fn user_by_id(&self, id: &str) -> Result<Option<UserRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT u.id, u.email, c.secret_hash
			 FROM users u JOIN user_credentials c ON c.user_id = u.id WHERE u.id = $1",
		)
		.bind(id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| UserRow {
			id: row.get("id"),
			email: row.get("email"),
			password_hash: row.get("secret_hash"),
		}))
	}

	pub async fn create_session(&self, session: &SessionRow) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO user_sessions (id, user_id, token_hash, created_at, last_used_at, idle_expires_at, absolute_expires_at)
			 VALUES ($1, $2, $3, $4, $4, $5, $6)",
		)
		.bind(&session.id)
		.bind(&session.user_id)
		.bind(&session.token_hash)
		.bind(session.created_at)
		.bind(session.idle_expires_at)
		.bind(session.absolute_expires_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn session_by_token(&self, token_hash: &[u8], now: i64) -> Result<Option<SessionRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT id, user_id, token_hash, created_at, idle_expires_at, absolute_expires_at FROM user_sessions
			 WHERE token_hash = $1 AND revoked_at IS NULL AND idle_expires_at > $2 AND absolute_expires_at > $2",
		)
		.bind(token_hash)
		.bind(now)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| SessionRow {
			id: row.get("id"),
			user_id: row.get("user_id"),
			token_hash: row.get("token_hash"),
			created_at: row.get("created_at"),
			idle_expires_at: row.get("idle_expires_at"),
			absolute_expires_at: row.get("absolute_expires_at"),
		}))
	}

	pub async fn touch_session(&self, id: &str, last_used_at: i64, idle_expires_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE user_sessions SET last_used_at = $1, idle_expires_at = $2 WHERE id = $3")
			.bind(last_used_at)
			.bind(idle_expires_at)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn revoke_session(&self, id: &str, revoked_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE user_sessions SET revoked_at = $1 WHERE id = $2")
			.bind(revoked_at)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn create_api_key(&self, key: &ApiKeyRow) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO api_keys (id, user_id, name, prefix, secret_hash, scopes, created_at, expires_at)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
		)
		.bind(&key.id)
		.bind(&key.user_id)
		.bind(&key.name)
		.bind(&key.prefix)
		.bind(&key.secret_hash)
		.bind(&key.scopes)
		.bind(key.created_at)
		.bind(key.expires_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn api_keys_for_user(&self, user_id: &str) -> Result<Vec<ApiKeyRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT id, user_id, name, prefix, scopes, created_at, expires_at, last_used_at
			 FROM api_keys WHERE user_id = $1 AND revoked_at IS NULL ORDER BY created_at",
		)
		.bind(user_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| ApiKeyRow {
				id: row.get("id"),
				user_id: row.get("user_id"),
				name: row.get("name"),
				prefix: row.get("prefix"),
				secret_hash: Vec::new(),
				scopes: row.get("scopes"),
				created_at: row.get("created_at"),
				expires_at: row.get("expires_at"),
				last_used_at: row.get("last_used_at"),
			})
			.collect())
	}

	pub async fn api_key_by_token(&self, token_hash: &[u8], now: i64) -> Result<Option<ApiKeyRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT id, user_id, name, prefix, secret_hash, scopes, created_at, expires_at, last_used_at FROM api_keys
			 WHERE secret_hash = $1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > $2)",
		)
		.bind(token_hash)
		.bind(now)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| ApiKeyRow {
			id: row.get("id"),
			user_id: row.get("user_id"),
			name: row.get("name"),
			prefix: row.get("prefix"),
			secret_hash: row.get("secret_hash"),
			scopes: row.get("scopes"),
			created_at: row.get("created_at"),
			expires_at: row.get("expires_at"),
			last_used_at: row.get("last_used_at"),
		}))
	}

	pub async fn touch_api_key(&self, id: &str, last_used_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE api_keys SET last_used_at = $1 WHERE id = $2")
			.bind(last_used_at)
			.bind(id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn revoke_api_key(&self, user_id: &str, key_id: &str, revoked_at: i64) -> Result<bool, sqlx::Error> {
		let result =
			sqlx::query("UPDATE api_keys SET revoked_at = $1 WHERE id = $2 AND user_id = $3 AND revoked_at IS NULL")
				.bind(revoked_at)
				.bind(key_id)
				.bind(user_id)
				.execute(&self.pool)
				.await?;
		Ok(result.rows_affected() == 1)
	}
}
