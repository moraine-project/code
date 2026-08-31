CREATE TABLE IF NOT EXISTS project_migrations (
	project_id TEXT NOT NULL,
	object_digest BLOB NOT NULL,
	old_home TEXT NOT NULL,
	new_home TEXT NOT NULL,
	cutover_seq INTEGER NOT NULL,
	reason TEXT,
	declared_time INTEGER NOT NULL,
	recorded_at INTEGER NOT NULL,
	PRIMARY KEY (project_id, object_digest)
);
