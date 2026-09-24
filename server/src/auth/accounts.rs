use sqlx::Row;

use crate::db::MetadataStore;

#[derive(Debug, Clone)]
pub struct UserRow {
	pub id: String,
	pub email: String,
	pub password_hash: String,
	pub role: String,
	pub created_at: i64,
	pub verified_at: Option<i64>,
}

pub struct AccountRow {
	pub id: String,
	pub email: String,
	pub role: String,
	pub created_at: i64,
	pub verified_at: Option<i64>,
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
	pub async fn create_user(
		&self,
		id: &str,
		email: &str,
		password_hash: &str,
		role: &str,
		created_at: i64,
	) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		sqlx::query("INSERT INTO users (id, email, role, created_at) VALUES ($1, $2, $3, $4)")
			.bind(id)
			.bind(email)
			.bind(role)
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
			"SELECT u.id, u.email, u.role, u.created_at, u.verified_at, c.secret_hash
			 FROM users u JOIN user_credentials c ON c.user_id = u.id WHERE u.email = $1",
		)
		.bind(email)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| UserRow {
			id: row.get("id"),
			email: row.get("email"),
			password_hash: row.get("secret_hash"),
			role: row.get("role"),
			created_at: row.get("created_at"),
			verified_at: row.get("verified_at"),
		}))
	}

	pub async fn user_by_id(&self, id: &str) -> Result<Option<UserRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT u.id, u.email, u.role, u.created_at, u.verified_at, c.secret_hash
			 FROM users u JOIN user_credentials c ON c.user_id = u.id WHERE u.id = $1",
		)
		.bind(id)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| UserRow {
			id: row.get("id"),
			email: row.get("email"),
			password_hash: row.get("secret_hash"),
			role: row.get("role"),
			created_at: row.get("created_at"),
			verified_at: row.get("verified_at"),
		}))
	}

	pub async fn user_verified(&self, id: &str) -> Result<bool, sqlx::Error> {
		let verified = sqlx::query_scalar::<_, Option<i64>>("SELECT verified_at FROM users WHERE id = $1")
			.bind(id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(matches!(verified, Some(Some(_))))
	}

	pub async fn list_users(&self, limit: i64) -> Result<Vec<AccountRow>, sqlx::Error> {
		let rows =
			sqlx::query("SELECT id, email, role, created_at, verified_at FROM users ORDER BY created_at, id LIMIT $1")
				.bind(limit)
				.fetch_all(&self.pool)
				.await?;
		Ok(rows
			.into_iter()
			.map(|row| AccountRow {
				id: row.get("id"),
				email: row.get("email"),
				role: row.get("role"),
				created_at: row.get("created_at"),
				verified_at: row.get("verified_at"),
			})
			.collect())
	}

	pub async fn set_user_verified(&self, user_id: &str, verified_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE users SET verified_at = $1 WHERE id = $2")
			.bind(verified_at)
			.bind(user_id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn last_owner_orgs(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT o.handle FROM org_members m JOIN orgs o ON o.id = m.org_id
			 WHERE m.user_id = $1 AND m.role = 'owner'
			 AND (SELECT COUNT(*) FROM org_members o2 WHERE o2.org_id = m.org_id AND o2.role = 'owner') <= 1",
		)
		.bind(user_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows.into_iter().map(|row| row.get("handle")).collect())
	}

	pub async fn delete_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		for statement in [
			"DELETE FROM recovery_codes WHERE user_id = $1",
			"DELETE FROM email_verifications WHERE user_id = $1",
			"DELETE FROM api_keys WHERE user_id = $1",
			"DELETE FROM user_sessions WHERE user_id = $1",
			"DELETE FROM follows WHERE user_id = $1",
			"DELETE FROM notifications WHERE user_id = $1",
			"DELETE FROM org_members WHERE user_id = $1",
			"DELETE FROM blob_uploads WHERE user_id = $1",
			"DELETE FROM user_credentials WHERE user_id = $1",
			"DELETE FROM users WHERE id = $1",
		] {
			sqlx::query(statement).bind(user_id).execute(&mut *transaction).await?;
		}
		transaction.commit().await?;
		Ok(())
	}

	pub async fn replace_email_verification(
		&self,
		user_id: &str,
		token_hash: &[u8],
		created_at: i64,
		expires_at: i64,
	) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		sqlx::query("DELETE FROM email_verifications WHERE user_id = $1")
			.bind(user_id)
			.execute(&mut *transaction)
			.await?;
		sqlx::query("INSERT INTO email_verifications (user_id, token_hash, created_at, expires_at) VALUES ($1, $2, $3, $4)")
			.bind(user_id)
			.bind(token_hash)
			.bind(created_at)
			.bind(expires_at)
			.execute(&mut *transaction)
			.await?;
		transaction.commit().await?;
		Ok(())
	}

	pub async fn consume_email_verification(&self, token_hash: &[u8], now: i64) -> Result<Option<String>, sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		let user_id = sqlx::query_scalar::<_, String>(
			"SELECT user_id FROM email_verifications WHERE token_hash = $1 AND expires_at > $2",
		)
		.bind(token_hash)
		.bind(now)
		.fetch_optional(&mut *transaction)
		.await?;
		if let Some(user_id) = &user_id {
			sqlx::query("DELETE FROM email_verifications WHERE user_id = $1")
				.bind(user_id)
				.execute(&mut *transaction)
				.await?;
		}
		transaction.commit().await?;
		Ok(user_id)
	}

	pub async fn user_role(&self, id: &str) -> Result<Option<String>, sqlx::Error> {
		let role = sqlx::query_scalar::<_, String>("SELECT role FROM users WHERE id = $1")
			.bind(id)
			.fetch_optional(&self.pool)
			.await?;
		Ok(role)
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

	pub async fn update_password(&self, user_id: &str, secret_hash: &str, updated_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE user_credentials SET secret_hash = $1, updated_at = $2 WHERE user_id = $3")
			.bind(secret_hash)
			.bind(updated_at)
			.bind(user_id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn revoke_sessions(&self, user_id: &str, revoked_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE user_sessions SET revoked_at = $1 WHERE user_id = $2 AND revoked_at IS NULL")
			.bind(revoked_at)
			.bind(user_id)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn revoke_other_sessions(&self, user_id: &str, keep: &str, revoked_at: i64) -> Result<(), sqlx::Error> {
		sqlx::query("UPDATE user_sessions SET revoked_at = $1 WHERE user_id = $2 AND id != $3 AND revoked_at IS NULL")
			.bind(revoked_at)
			.bind(user_id)
			.bind(keep)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn replace_recovery_codes(
		&self,
		user_id: &str,
		hashes: &[Vec<u8>],
		created_at: i64,
	) -> Result<(), sqlx::Error> {
		let mut transaction = self.pool.begin().await?;
		sqlx::query("DELETE FROM recovery_codes WHERE user_id = $1")
			.bind(user_id)
			.execute(&mut *transaction)
			.await?;
		for hash in hashes {
			sqlx::query("INSERT INTO recovery_codes (user_id, code_hash, created_at) VALUES ($1, $2, $3)")
				.bind(user_id)
				.bind(hash)
				.bind(created_at)
				.execute(&mut *transaction)
				.await?;
		}
		transaction.commit().await?;
		Ok(())
	}

	pub async fn claim_recovery_code(&self, user_id: &str, hash: &[u8], used_at: i64) -> Result<bool, sqlx::Error> {
		let result =
			sqlx::query("UPDATE recovery_codes SET used_at = $1 WHERE user_id = $2 AND code_hash = $3 AND used_at IS NULL")
				.bind(used_at)
				.bind(user_id)
				.bind(hash)
				.execute(&self.pool)
				.await?;
		Ok(result.rows_affected() == 1)
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

#[cfg(test)]
mod recovery_claim_tests {
	use crate::db::MetadataStore;

	#[tokio::test]
	async fn a_recovery_code_can_only_be_spent_once() {
		let directory = tempfile::tempdir().expect("tempdir");
		let store = MetadataStore::open_url(&crate::db::sqlite_url(&directory.path().join("metadata.sqlite")))
			.await
			.expect("store");
		let hash = crate::auth::password::hash_password("correct horse battery").expect("hash");
		store
			.create_user("u", "user@example.org", &hash, "user", 1)
			.await
			.expect("user");
		store.replace_recovery_codes("u", &[vec![7u8; 32]], 1).await.expect("codes");

		assert!(store.claim_recovery_code("u", &[7u8; 32], 2).await.expect("claim"));
		assert!(
			!store.claim_recovery_code("u", &[7u8; 32], 3).await.expect("claim"),
			"a spent recovery code must not be claimable again"
		);
		assert!(!store.claim_recovery_code("u", &[8u8; 32], 4).await.expect("claim"));
	}
}
