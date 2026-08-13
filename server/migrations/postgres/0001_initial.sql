CREATE TABLE IF NOT EXISTS objects (
	digest BYTEA PRIMARY KEY,
	kind TEXT NOT NULL,
	payload BYTEA NOT NULL,
	wire BYTEA NOT NULL
);
CREATE TABLE IF NOT EXISTS projects (
	id TEXT PRIMARY KEY,
	genesis_digest BYTEA NOT NULL,
	head_seq BIGINT NOT NULL DEFAULT 0,
	head_digest BYTEA,
	profile_digest BYTEA,
	owner_kind TEXT,
	owner_id TEXT
);
CREATE TABLE IF NOT EXISTS feed_entries (
	project_id TEXT NOT NULL,
	seq BIGINT NOT NULL,
	previous BYTEA,
	entry_digest BYTEA NOT NULL,
	kind TEXT NOT NULL,
	object_digest BYTEA NOT NULL,
	payload BYTEA NOT NULL,
	wire BYTEA NOT NULL,
	PRIMARY KEY (project_id, seq)
);
CREATE TABLE IF NOT EXISTS users (
	id TEXT PRIMARY KEY,
	email TEXT NOT NULL UNIQUE,
	created_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS user_credentials (
	user_id TEXT PRIMARY KEY,
	secret_hash TEXT NOT NULL,
	updated_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS user_sessions (
	id TEXT PRIMARY KEY,
	user_id TEXT NOT NULL,
	token_hash BYTEA NOT NULL UNIQUE,
	created_at BIGINT NOT NULL,
	last_used_at BIGINT NOT NULL,
	idle_expires_at BIGINT NOT NULL,
	absolute_expires_at BIGINT NOT NULL,
	revoked_at BIGINT
);
CREATE TABLE IF NOT EXISTS api_keys (
	id TEXT PRIMARY KEY,
	user_id TEXT NOT NULL,
	name TEXT NOT NULL,
	prefix TEXT NOT NULL,
	secret_hash BYTEA NOT NULL UNIQUE,
	scopes TEXT NOT NULL,
	created_at BIGINT NOT NULL,
	expires_at BIGINT,
	revoked_at BIGINT,
	last_used_at BIGINT
);
CREATE TABLE IF NOT EXISTS submissions (
	id TEXT PRIMARY KEY,
	project_id TEXT NOT NULL,
	object_digest BYTEA NOT NULL,
	entry_digest BYTEA NOT NULL,
	entry_wire BYTEA NOT NULL,
	state TEXT NOT NULL,
	assigned_to TEXT,
	submitted_by TEXT NOT NULL,
	created_at BIGINT NOT NULL,
	updated_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS orgs (
	id TEXT PRIMARY KEY,
	handle TEXT NOT NULL UNIQUE,
	display_name TEXT NOT NULL,
	created_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS org_members (
	org_id TEXT NOT NULL,
	user_id TEXT NOT NULL,
	role TEXT NOT NULL,
	added_at BIGINT NOT NULL,
	PRIMARY KEY (org_id, user_id)
);
CREATE TABLE IF NOT EXISTS teams (
	id TEXT PRIMARY KEY,
	org_id TEXT NOT NULL,
	parent_team_id TEXT,
	display_name TEXT NOT NULL,
	created_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS search_documents (
	project_id TEXT PRIMARY KEY,
	game_id TEXT NOT NULL,
	display_name TEXT NOT NULL,
	summary TEXT NOT NULL,
	updated_at BIGINT NOT NULL,
	created_at BIGINT NOT NULL DEFAULT 0,
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
	updated_at BIGINT NOT NULL,
	PRIMARY KEY (home_url, id)
);
CREATE TABLE IF NOT EXISTS definitions (
	id TEXT PRIMARY KEY,
	kind TEXT NOT NULL,
	genesis_digest BYTEA NOT NULL,
	current_digest BYTEA,
	created_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS webhooks (
	id TEXT PRIMARY KEY,
	owner_id TEXT NOT NULL,
	url TEXT NOT NULL,
	event_kinds TEXT NOT NULL,
	created_at BIGINT NOT NULL,
	revoked_at BIGINT
);
CREATE TABLE IF NOT EXISTS webhook_deliveries (
	id TEXT PRIMARY KEY,
	webhook_id TEXT NOT NULL,
	event_id TEXT NOT NULL UNIQUE,
	url TEXT NOT NULL,
	body TEXT NOT NULL,
	attempt BIGINT NOT NULL DEFAULT 0,
	status TEXT NOT NULL,
	next_attempt_at BIGINT NOT NULL,
	created_at BIGINT NOT NULL,
	delivered_at BIGINT
);
CREATE TABLE IF NOT EXISTS follows (
	user_id TEXT NOT NULL,
	project_id TEXT NOT NULL,
	created_at BIGINT NOT NULL,
	PRIMARY KEY (user_id, project_id)
);
CREATE TABLE IF NOT EXISTS notifications (
	id TEXT PRIMARY KEY,
	user_id TEXT NOT NULL,
	project_id TEXT NOT NULL,
	event_kind TEXT NOT NULL,
	object_digest BYTEA,
	feed_seq BIGINT,
	created_at BIGINT NOT NULL,
	read_at BIGINT
);
CREATE TABLE IF NOT EXISTS locations (
	artifact_digest BYTEA NOT NULL,
	url TEXT NOT NULL,
	kind TEXT NOT NULL,
	operator_id TEXT,
	object_digest BYTEA NOT NULL,
	PRIMARY KEY (artifact_digest, url)
);
CREATE TABLE IF NOT EXISTS mirrors (
	mirror_id TEXT PRIMARY KEY,
	public_key BYTEA NOT NULL,
	added_by TEXT NOT NULL,
	added_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS mirror_commitments (
	artifact_digest BYTEA NOT NULL,
	mirror_id TEXT NOT NULL,
	size BIGINT NOT NULL,
	accepted_at BIGINT NOT NULL,
	retention_until BIGINT,
	endpoint TEXT NOT NULL,
	object_digest BYTEA NOT NULL,
	PRIMARY KEY (artifact_digest, mirror_id)
);
CREATE TABLE IF NOT EXISTS providers (
	provider_id TEXT PRIMARY KEY,
	public_key BYTEA NOT NULL,
	added_by TEXT NOT NULL,
	added_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS advisories (
	digest BYTEA PRIMARY KEY,
	provider_id TEXT NOT NULL,
	project_id TEXT NOT NULL,
	game_id TEXT NOT NULL,
	affected_digest BYTEA,
	severity TEXT NOT NULL,
	category TEXT NOT NULL,
	block_promotion BIGINT NOT NULL,
	published_at BIGINT NOT NULL,
	retracted_at BIGINT
);
CREATE TABLE IF NOT EXISTS withdrawals (
	project_id TEXT NOT NULL,
	release_id TEXT NOT NULL,
	reason TEXT NOT NULL,
	note TEXT,
	declared_time BIGINT NOT NULL,
	PRIMARY KEY (project_id, release_id)
);
CREATE TABLE IF NOT EXISTS artifact_index (
	digest BYTEA NOT NULL,
	project_id TEXT NOT NULL,
	release_digest BYTEA NOT NULL,
	PRIMARY KEY (digest, release_digest)
);
CREATE TABLE IF NOT EXISTS download_counts (
	project_id TEXT NOT NULL,
	day BIGINT NOT NULL,
	count BIGINT NOT NULL,
	PRIMARY KEY (project_id, day)
);

CREATE TABLE IF NOT EXISTS subscriptions (
	home_url TEXT NOT NULL,
	project_id TEXT NOT NULL,
	cursor_seq BIGINT NOT NULL DEFAULT 0,
	remote_head_seq BIGINT NOT NULL DEFAULT 0,
	reset_count BIGINT NOT NULL DEFAULT 0,
	status TEXT NOT NULL,
	updated_at BIGINT NOT NULL,
	PRIMARY KEY (home_url, project_id)
);
CREATE TABLE IF NOT EXISTS review_decisions (
	id TEXT PRIMARY KEY,
	submission_id TEXT NOT NULL,
	object_digest BYTEA NOT NULL,
	reviewer_id TEXT NOT NULL,
	decision TEXT NOT NULL,
	reason_code TEXT,
	reason_taxonomy_version BIGINT NOT NULL,
	decided_at BIGINT NOT NULL,
	appeal_route TEXT
);
