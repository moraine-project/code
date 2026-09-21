CREATE TABLE IF NOT EXISTS external_projects (
	provider TEXT NOT NULL,
	external_project_id TEXT NOT NULL,
	source_class TEXT NOT NULL,
	canonical_source_url TEXT NOT NULL,
	observed_profile TEXT NOT NULL,
	observed_at BIGINT NOT NULL,
	last_synced_at BIGINT NOT NULL,
	source_state TEXT NOT NULL,
	bridge_id TEXT NOT NULL,
	bridge_version TEXT NOT NULL,
	linked_native_project_id TEXT,
	PRIMARY KEY (provider, external_project_id)
);
CREATE TABLE IF NOT EXISTS external_files (
	provider TEXT NOT NULL,
	external_file_id TEXT NOT NULL,
	external_project_id TEXT NOT NULL,
	source_url TEXT NOT NULL,
	digest BYTEA,
	size BIGINT,
	metadata_json TEXT NOT NULL,
	observed_at BIGINT NOT NULL,
	deleted_at BIGINT,
	PRIMARY KEY (provider, external_file_id)
);
CREATE INDEX IF NOT EXISTS external_files_by_project ON external_files(provider, external_project_id);
CREATE TABLE IF NOT EXISTS external_project_claims (
	id TEXT PRIMARY KEY,
	provider TEXT NOT NULL,
	external_project_id TEXT NOT NULL,
	claimant_ref TEXT NOT NULL,
	challenge_ref TEXT NOT NULL,
	state TEXT NOT NULL,
	recorded_by TEXT NOT NULL,
	recorded_at BIGINT NOT NULL,
	expires_at BIGINT NOT NULL,
	verified_at BIGINT,
	reviewed_by TEXT
);
CREATE INDEX IF NOT EXISTS external_claims_by_project ON external_project_claims(provider, external_project_id);
