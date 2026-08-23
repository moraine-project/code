CREATE TABLE IF NOT EXISTS loader_releases (
	loader_id TEXT NOT NULL,
	version_id TEXT NOT NULL,
	object_digest BYTEA NOT NULL,
	declared_time BIGINT NOT NULL,
	PRIMARY KEY (loader_id, version_id)
);
