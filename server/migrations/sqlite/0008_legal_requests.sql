CREATE TABLE IF NOT EXISTS legal_requests (
	id TEXT PRIMARY KEY,
	kind TEXT NOT NULL,
	claimant_ref TEXT NOT NULL,
	target_kind TEXT NOT NULL,
	target_id TEXT NOT NULL,
	stated_basis TEXT NOT NULL,
	received_at INTEGER NOT NULL,
	action_taken TEXT NOT NULL,
	designated_agent_ref TEXT,
	responds_to TEXT,
	recorded_by TEXT NOT NULL,
	recorded_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS legal_requests_by_target ON legal_requests (target_kind, target_id);
