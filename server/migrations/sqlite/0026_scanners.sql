CREATE TABLE IF NOT EXISTS scanner_providers (
	provider_id TEXT PRIMARY KEY,
	kind TEXT NOT NULL,
	command TEXT NOT NULL,
	args_json TEXT NOT NULL,
	public_key BLOB NOT NULL,
	enabled INTEGER NOT NULL DEFAULT 1,
	created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS scan_policies (
	id TEXT PRIMARY KEY,
	provider_id TEXT NOT NULL,
	enabled INTEGER NOT NULL DEFAULT 1,
	auto_scan INTEGER NOT NULL DEFAULT 1,
	created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS scan_jobs (
	id TEXT PRIMARY KEY,
	provider_id TEXT NOT NULL,
	artifact_digest BLOB NOT NULL,
	status TEXT NOT NULL,
	requested_by TEXT NOT NULL,
	attempts INTEGER NOT NULL DEFAULT 0,
	result_json TEXT,
	error TEXT,
	created_at INTEGER NOT NULL,
	started_at INTEGER,
	finished_at INTEGER,
	UNIQUE(provider_id, artifact_digest)
);
CREATE INDEX IF NOT EXISTS scan_jobs_pending ON scan_jobs(status, created_at);
CREATE TABLE IF NOT EXISTS scanner_subscriptions (
	id TEXT PRIMARY KEY,
	provider_id TEXT NOT NULL,
	endpoint TEXT NOT NULL,
	interval_seconds INTEGER NOT NULL DEFAULT 3600,
	enabled INTEGER NOT NULL DEFAULT 1,
	last_polled_at INTEGER,
	created_at INTEGER NOT NULL
);
