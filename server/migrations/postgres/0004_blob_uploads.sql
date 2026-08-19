CREATE TABLE IF NOT EXISTS blob_uploads (
	artifact_digest BYTEA PRIMARY KEY,
	user_id TEXT NOT NULL,
	size BIGINT NOT NULL,
	created_at BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS blob_uploads_by_user ON blob_uploads (user_id);
