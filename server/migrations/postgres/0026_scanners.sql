CREATE TABLE IF NOT EXISTS scanner_providers (
	provider_id TEXT PRIMARY KEY,
	kind TEXT NOT NULL,
	command TEXT NOT NULL,
	args_json TEXT NOT NULL,
	public_key BYTEA NOT NULL,
	enabled BOOLEAN NOT NULL DEFAULT TRUE,
	created_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS scan_policies (
	id TEXT PRIMARY KEY,
	provider_id TEXT NOT NULL,
	enabled BOOLEAN NOT NULL DEFAULT TRUE,
	auto_scan BOOLEAN NOT NULL DEFAULT TRUE,
	created_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS scan_jobs (
	id TEXT PRIMARY KEY,
	provider_id TEXT NOT NULL,
	artifact_digest BYTEA NOT NULL,
	status TEXT NOT NULL,
	requested_by TEXT NOT NULL,
	attempts BIGINT NOT NULL DEFAULT 0,
	result_json TEXT,
	error TEXT,
	created_at BIGINT NOT NULL,
	started_at BIGINT,
	finished_at BIGINT,
	UNIQUE(provider_id, artifact_digest)
);
CREATE INDEX IF NOT EXISTS scan_jobs_pending ON scan_jobs(status, created_at);
CREATE TABLE IF NOT EXISTS scanner_subscriptions (
	id TEXT PRIMARY KEY,
	provider_id TEXT NOT NULL,
	endpoint TEXT NOT NULL,
	interval_seconds BIGINT NOT NULL DEFAULT 3600,
	enabled BOOLEAN NOT NULL DEFAULT TRUE,
	last_polled_at BIGINT,
	created_at BIGINT NOT NULL
);
