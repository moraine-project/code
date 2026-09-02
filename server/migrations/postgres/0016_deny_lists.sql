CREATE TABLE IF NOT EXISTS deny_list_entries (
	issuer_id TEXT NOT NULL,
	target_kind TEXT NOT NULL,
	target_id TEXT NOT NULL,
	reason_code TEXT NOT NULL,
	reason_taxonomy_version BIGINT NOT NULL,
	scope_kind TEXT NOT NULL,
	scope_id TEXT NOT NULL,
	valid_from BIGINT,
	valid_until BIGINT,
	object_digest BYTEA NOT NULL,
	issued_at BIGINT NOT NULL,
	PRIMARY KEY (issuer_id, target_kind, target_id, scope_kind, scope_id)
);
CREATE INDEX IF NOT EXISTS deny_list_entries_by_target ON deny_list_entries (target_kind, target_id);
