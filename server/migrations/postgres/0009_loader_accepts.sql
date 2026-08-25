CREATE TABLE IF NOT EXISTS loader_accepts (
	accepting_loader_id TEXT NOT NULL,
	accepted_loader_id TEXT NOT NULL,
	object_digest BYTEA NOT NULL,
	qualification TEXT NOT NULL,
	declared_by_kind TEXT NOT NULL,
	declared_by_id TEXT NOT NULL,
	declared_time BIGINT NOT NULL,
	PRIMARY KEY (accepting_loader_id, accepted_loader_id)
);
