CREATE TABLE IF NOT EXISTS recovery_codes (
	user_id TEXT NOT NULL,
	code_hash BYTEA NOT NULL,
	created_at INTEGER NOT NULL,
	used_at INTEGER,
	PRIMARY KEY (user_id, code_hash)
);
