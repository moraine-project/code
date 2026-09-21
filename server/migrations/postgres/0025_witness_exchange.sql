CREATE TABLE IF NOT EXISTS witness_exchanges (
	project_id TEXT NOT NULL,
	observer_id TEXT NOT NULL,
	source_home TEXT NOT NULL,
	sequence BIGINT NOT NULL,
	head_entry TEXT NOT NULL,
	observed_at BIGINT NOT NULL,
	PRIMARY KEY (project_id, observer_id, source_home, sequence, head_entry)
);
CREATE INDEX IF NOT EXISTS witness_exchanges_by_project ON witness_exchanges (project_id, sequence);
