CREATE TABLE IF NOT EXISTS witness_observations (
	project_id TEXT NOT NULL,
	source_home TEXT NOT NULL,
	sequence BIGINT NOT NULL,
	head_entry TEXT NOT NULL,
	observed_at BIGINT NOT NULL,
	PRIMARY KEY (project_id, source_home, sequence, head_entry)
);
CREATE INDEX IF NOT EXISTS witness_observations_by_project ON witness_observations (project_id, sequence);
