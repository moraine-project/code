CREATE TABLE IF NOT EXISTS blob_uploads (
	artifact_digest BLOB PRIMARY KEY,
	user_id TEXT NOT NULL,
	size INTEGER NOT NULL,
	created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS blob_uploads_by_user ON blob_uploads (user_id);
