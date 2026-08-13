CREATE TABLE IF NOT EXISTS objects (
	digest BLOB PRIMARY KEY,
	kind TEXT NOT NULL,
	payload BLOB NOT NULL,
	wire BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS projects (
	id TEXT PRIMARY KEY,
	genesis_digest BLOB NOT NULL,
	head_seq INTEGER NOT NULL DEFAULT 0,
	head_digest BLOB,
	profile_digest BLOB,
	owner_kind TEXT,
	owner_id TEXT
);
CREATE TABLE IF NOT EXISTS feed_entries (
	project_id TEXT NOT NULL,
	seq INTEGER NOT NULL,
	previous BLOB,
	entry_digest BLOB NOT NULL,
	kind TEXT NOT NULL,
	object_digest BLOB NOT NULL,
	payload BLOB NOT NULL,
	wire BLOB NOT NULL,
	PRIMARY KEY (project_id, seq)
);
CREATE TABLE IF NOT EXISTS users (
	id TEXT PRIMARY KEY,
	email TEXT NOT NULL UNIQUE,
	created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS user_credentials (
	user_id TEXT PRIMARY KEY,
	secret_hash TEXT NOT NULL,
	updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS user_sessions (
	id TEXT PRIMARY KEY,
	user_id TEXT NOT NULL,
	token_hash BLOB NOT NULL UNIQUE,
	created_at INTEGER NOT NULL,
	last_used_at INTEGER NOT NULL,
	idle_expires_at INTEGER NOT NULL,
	absolute_expires_at INTEGER NOT NULL,
	revoked_at INTEGER
);
CREATE TABLE IF NOT EXISTS api_keys (
	id TEXT PRIMARY KEY,
	user_id TEXT NOT NULL,
	name TEXT NOT NULL,
	prefix TEXT NOT NULL,
	secret_hash BLOB NOT NULL UNIQUE,
	scopes TEXT NOT NULL,
	created_at INTEGER NOT NULL,
	expires_at INTEGER,
	revoked_at INTEGER,
	last_used_at INTEGER
);
CREATE TABLE IF NOT EXISTS submissions (
	id TEXT PRIMARY KEY,
	project_id TEXT NOT NULL,
	object_digest BLOB NOT NULL,
	entry_digest BLOB NOT NULL,
	entry_wire BLOB NOT NULL,
	state TEXT NOT NULL,
	assigned_to TEXT,
	submitted_by TEXT NOT NULL,
	created_at INTEGER NOT NULL,
	updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS orgs (
	id TEXT PRIMARY KEY,
	handle TEXT NOT NULL UNIQUE,
	display_name TEXT NOT NULL,
	created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS org_members (
	org_id TEXT NOT NULL,
	user_id TEXT NOT NULL,
	role TEXT NOT NULL,
	added_at INTEGER NOT NULL,
	PRIMARY KEY (org_id, user_id)
);
CREATE TABLE IF NOT EXISTS teams (
	id TEXT PRIMARY KEY,
	org_id TEXT NOT NULL,
	parent_team_id TEXT,
	display_name TEXT NOT NULL,
	created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS search_documents (
	project_id TEXT PRIMARY KEY,
	game_id TEXT NOT NULL,
	display_name TEXT NOT NULL,
	summary TEXT NOT NULL,
	updated_at INTEGER NOT NULL,
	created_at INTEGER NOT NULL DEFAULT 0,
	normalized_name TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS search_labels (
	project_id TEXT NOT NULL,
	label_kind TEXT NOT NULL,
	label_id TEXT NOT NULL,
	PRIMARY KEY (project_id, label_kind, label_id)
);
CREATE TABLE IF NOT EXISTS definition_subscriptions (
	home_url TEXT NOT NULL,
	id TEXT NOT NULL,
	kind TEXT NOT NULL,
	updated_at INTEGER NOT NULL,
	PRIMARY KEY (home_url, id)
);
CREATE TABLE IF NOT EXISTS definitions (
	id TEXT PRIMARY KEY,
	kind TEXT NOT NULL,
	genesis_digest BLOB NOT NULL,
	current_digest BLOB,
	created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS webhooks (
	id TEXT PRIMARY KEY,
	owner_id TEXT NOT NULL,
	url TEXT NOT NULL,
	event_kinds TEXT NOT NULL,
	created_at INTEGER NOT NULL,
	revoked_at INTEGER
);
CREATE TABLE IF NOT EXISTS webhook_deliveries (
	id TEXT PRIMARY KEY,
	webhook_id TEXT NOT NULL,
	event_id TEXT NOT NULL UNIQUE,
	url TEXT NOT NULL,
	body TEXT NOT NULL,
	attempt INTEGER NOT NULL DEFAULT 0,
	status TEXT NOT NULL,
	next_attempt_at INTEGER NOT NULL,
	created_at INTEGER NOT NULL,
	delivered_at INTEGER
);
CREATE TABLE IF NOT EXISTS follows (
	user_id TEXT NOT NULL,
	project_id TEXT NOT NULL,
	created_at INTEGER NOT NULL,
	PRIMARY KEY (user_id, project_id)
);
CREATE TABLE IF NOT EXISTS notifications (
	id TEXT PRIMARY KEY,
	user_id TEXT NOT NULL,
	project_id TEXT NOT NULL,
	event_kind TEXT NOT NULL,
	object_digest BLOB,
	feed_seq INTEGER,
	created_at INTEGER NOT NULL,
	read_at INTEGER
);
CREATE TABLE IF NOT EXISTS locations (
	artifact_digest BLOB NOT NULL,
	url TEXT NOT NULL,
	kind TEXT NOT NULL,
	operator_id TEXT,
	object_digest BLOB NOT NULL,
	PRIMARY KEY (artifact_digest, url)
);
CREATE TABLE IF NOT EXISTS mirrors (
	mirror_id TEXT PRIMARY KEY,
	public_key BLOB NOT NULL,
	added_by TEXT NOT NULL,
	added_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS mirror_commitments (
	artifact_digest BLOB NOT NULL,
	mirror_id TEXT NOT NULL,
	size INTEGER NOT NULL,
	accepted_at INTEGER NOT NULL,
	retention_until INTEGER,
	endpoint TEXT NOT NULL,
	object_digest BLOB NOT NULL,
	PRIMARY KEY (artifact_digest, mirror_id)
);
CREATE TABLE IF NOT EXISTS providers (
	provider_id TEXT PRIMARY KEY,
	public_key BLOB NOT NULL,
	added_by TEXT NOT NULL,
	added_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS advisories (
	digest BLOB PRIMARY KEY,
	provider_id TEXT NOT NULL,
	project_id TEXT NOT NULL,
	game_id TEXT NOT NULL,
	affected_digest BLOB,
	severity TEXT NOT NULL,
	category TEXT NOT NULL,
	block_promotion INTEGER NOT NULL,
	published_at INTEGER NOT NULL,
	retracted_at INTEGER
);
CREATE TABLE IF NOT EXISTS withdrawals (
	project_id TEXT NOT NULL,
	release_id TEXT NOT NULL,
	reason TEXT NOT NULL,
	note TEXT,
	declared_time INTEGER NOT NULL,
	PRIMARY KEY (project_id, release_id)
);
CREATE TABLE IF NOT EXISTS artifact_index (
	digest BLOB NOT NULL,
	project_id TEXT NOT NULL,
	release_digest BLOB NOT NULL,
	PRIMARY KEY (digest, release_digest)
);
CREATE TABLE IF NOT EXISTS download_counts (
	project_id TEXT NOT NULL,
	day INTEGER NOT NULL,
	count INTEGER NOT NULL,
	PRIMARY KEY (project_id, day)
);

CREATE TABLE IF NOT EXISTS subscriptions (
	home_url TEXT NOT NULL,
	project_id TEXT NOT NULL,
	cursor_seq INTEGER NOT NULL DEFAULT 0,
	remote_head_seq INTEGER NOT NULL DEFAULT 0,
	reset_count INTEGER NOT NULL DEFAULT 0,
	status TEXT NOT NULL,
	updated_at INTEGER NOT NULL,
	PRIMARY KEY (home_url, project_id)
);
CREATE TABLE IF NOT EXISTS review_decisions (
	id TEXT PRIMARY KEY,
	submission_id TEXT NOT NULL,
	object_digest BLOB NOT NULL,
	reviewer_id TEXT NOT NULL,
	decision TEXT NOT NULL,
	reason_code TEXT,
	reason_taxonomy_version INTEGER NOT NULL,
	decided_at INTEGER NOT NULL,
	appeal_route TEXT
);
