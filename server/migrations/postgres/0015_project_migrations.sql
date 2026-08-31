CREATE TABLE IF NOT EXISTS project_migrations (
	project_id TEXT NOT NULL,
	object_digest BYTEA NOT NULL,
	old_home TEXT NOT NULL,
	new_home TEXT NOT NULL,
	cutover_seq BIGINT NOT NULL,
	reason TEXT,
	declared_time BIGINT NOT NULL,
	recorded_at BIGINT NOT NULL,
	PRIMARY KEY (project_id, object_digest)
);
