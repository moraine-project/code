CREATE TABLE IF NOT EXISTS mirror_confirmations (
	artifact_digest BLOB NOT NULL,
	mirror_id TEXT NOT NULL,
	checked_at INTEGER NOT NULL,
	reachable INTEGER NOT NULL,
	PRIMARY KEY (artifact_digest, mirror_id)
);
