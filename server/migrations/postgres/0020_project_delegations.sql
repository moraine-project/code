CREATE TABLE IF NOT EXISTS project_delegations (
	project_id TEXT NOT NULL,
	digest BYTEA NOT NULL,
	PRIMARY KEY (project_id, digest)
);
