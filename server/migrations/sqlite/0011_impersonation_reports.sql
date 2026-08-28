CREATE TABLE IF NOT EXISTS impersonation_reports (
	id TEXT PRIMARY KEY,
	claim_kind TEXT NOT NULL,
	claimant_ref TEXT NOT NULL,
	target_project_id TEXT,
	target_handle TEXT,
	evidence_ref TEXT NOT NULL,
	status TEXT NOT NULL,
	decided_at INTEGER,
	recorded_by TEXT NOT NULL,
	recorded_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS impersonation_reports_by_project ON impersonation_reports (target_project_id);
CREATE INDEX IF NOT EXISTS impersonation_reports_by_handle ON impersonation_reports (target_handle);
