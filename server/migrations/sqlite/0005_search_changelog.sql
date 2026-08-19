CREATE TABLE IF NOT EXISTS search_changelog_text (
	object_digest BLOB PRIMARY KEY,
	project_id TEXT NOT NULL,
	text TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS search_changelog_text_by_project ON search_changelog_text (project_id);
