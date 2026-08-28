CREATE TABLE IF NOT EXISTS sanctions (
	id TEXT PRIMARY KEY,
	subject_user_id TEXT NOT NULL,
	org_id TEXT,
	kind TEXT NOT NULL,
	reason_code TEXT NOT NULL,
	reason_taxonomy_version INTEGER NOT NULL,
	scope_kind TEXT NOT NULL,
	scope_id TEXT NOT NULL,
	starts_at INTEGER NOT NULL,
	expires_at INTEGER,
	decided_by TEXT NOT NULL,
	recorded_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS sanctions_by_subject ON sanctions (subject_user_id, starts_at);
