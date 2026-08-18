CREATE TABLE IF NOT EXISTS mirror_confirmations (
	artifact_digest BYTEA NOT NULL,
	mirror_id TEXT NOT NULL,
	checked_at BIGINT NOT NULL,
	reachable BIGINT NOT NULL,
	PRIMARY KEY (artifact_digest, mirror_id)
);
