ALTER TABLE users ADD COLUMN verified_at INTEGER;
UPDATE users SET verified_at = created_at WHERE verified_at IS NULL;
CREATE TABLE IF NOT EXISTS email_verifications (
	user_id TEXT NOT NULL,
	token_hash BYTEA NOT NULL,
	created_at INTEGER NOT NULL,
	expires_at INTEGER NOT NULL,
	PRIMARY KEY (user_id, token_hash)
);
