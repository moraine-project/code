CREATE TABLE IF NOT EXISTS loader_releases (
	loader_id TEXT NOT NULL,
	version_id TEXT NOT NULL,
	object_digest BLOB NOT NULL,
	declared_time INTEGER NOT NULL,
	PRIMARY KEY (loader_id, version_id)
);
