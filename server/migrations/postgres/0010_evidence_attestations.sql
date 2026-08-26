CREATE TABLE IF NOT EXISTS evidence_attestations (
	object_digest BYTEA PRIMARY KEY,
	artifact_digest BYTEA NOT NULL,
	kind TEXT NOT NULL,
	signer_id TEXT NOT NULL,
	subject_kind TEXT NOT NULL,
	subject_id TEXT NOT NULL,
	media_type TEXT NOT NULL,
	issued_at BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS evidence_attestations_by_artifact ON evidence_attestations (artifact_digest, kind);
