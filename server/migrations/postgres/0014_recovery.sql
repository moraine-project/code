CREATE TABLE IF NOT EXISTS project_roots (
	project_id TEXT PRIMARY KEY,
	roots TEXT NOT NULL,
	threshold BIGINT NOT NULL,
	valid_from_seq BIGINT NOT NULL,
	updated_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS recovery_claims (
	project_id TEXT NOT NULL,
	object_digest BYTEA NOT NULL,
	valid_from_seq BIGINT NOT NULL,
	roots TEXT NOT NULL,
	applied BIGINT NOT NULL,
	recorded_at BIGINT NOT NULL,
	PRIMARY KEY (project_id, object_digest)
);
