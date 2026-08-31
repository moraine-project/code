CREATE TABLE IF NOT EXISTS project_roots (
	project_id TEXT PRIMARY KEY,
	roots TEXT NOT NULL,
	threshold INTEGER NOT NULL,
	valid_from_seq INTEGER NOT NULL,
	updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS recovery_claims (
	project_id TEXT NOT NULL,
	object_digest BLOB NOT NULL,
	valid_from_seq INTEGER NOT NULL,
	roots TEXT NOT NULL,
	applied INTEGER NOT NULL,
	recorded_at INTEGER NOT NULL,
	PRIMARY KEY (project_id, object_digest)
);
