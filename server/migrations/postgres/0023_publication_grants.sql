CREATE TABLE IF NOT EXISTS publication_grants (
	id TEXT PRIMARY KEY,
	project_id TEXT NOT NULL,
	principal_kind TEXT NOT NULL,
	principal_id TEXT NOT NULL,
	game_id TEXT NOT NULL,
	release_kinds TEXT NOT NULL,
	artifact_kinds TEXT NOT NULL,
	issued_from_review TEXT NOT NULL,
	policy_version TEXT NOT NULL,
	issued_at BIGINT NOT NULL,
	expires_at BIGINT,
	suspended_at BIGINT,
	revoked_at BIGINT,
	reason_code TEXT
);
CREATE INDEX IF NOT EXISTS publication_grants_active ON publication_grants(project_id, principal_kind, principal_id, revoked_at, suspended_at);
