CREATE TABLE IF NOT EXISTS directory_policy (
	project_id TEXT PRIMARY KEY,
	listing_state TEXT NOT NULL,
	reason_code TEXT,
	reason_note TEXT,
	updated_at BIGINT NOT NULL
);
