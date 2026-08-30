CREATE TABLE IF NOT EXISTS search_trigrams (
	trigram TEXT NOT NULL,
	project_id TEXT NOT NULL,
	weight BIGINT NOT NULL,
	PRIMARY KEY (trigram, project_id)
);
