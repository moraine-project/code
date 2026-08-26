CREATE TABLE IF NOT EXISTS evidence_attestations (
	object_digest BLOB PRIMARY KEY,
	artifact_digest BLOB NOT NULL,
	kind TEXT NOT NULL,
	signer_id TEXT NOT NULL,
	subject_kind TEXT NOT NULL,
	subject_id TEXT NOT NULL,
	media_type TEXT NOT NULL,
	issued_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS evidence_attestations_by_artifact ON evidence_attestations (artifact_digest, kind);
