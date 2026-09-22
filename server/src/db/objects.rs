use sqlx::Row;

use super::{FeedRow, MetadataStore, StoredObject};

impl MetadataStore {
	pub async fn put_object(&self, object: &StoredObject) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT INTO objects (digest, kind, payload, wire) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING")
			.bind(&object.digest)
			.bind(&object.kind)
			.bind(&object.payload)
			.bind(&object.wire)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn object(&self, digest: &[u8]) -> Result<Option<StoredObject>, sqlx::Error> {
		let row = sqlx::query("SELECT digest, kind, payload, wire FROM objects WHERE digest = $1")
			.bind(digest)
			.fetch_optional(&self.pool)
			.await?;
		Ok(row.map(|row| StoredObject {
			digest: row.get("digest"),
			kind: row.get("kind"),
			payload: row.get("payload"),
			wire: row.get("wire"),
		}))
	}

	pub async fn feed_entry_for_object(
		&self,
		project_id: &str,
		object_digest: &[u8],
	) -> Result<Option<FeedRow>, sqlx::Error> {
		let row = sqlx::query(
			"SELECT project_id, seq, previous, entry_digest, kind, object_digest, payload, wire FROM feed_entries
			 WHERE project_id = $1 AND object_digest = $2 ORDER BY seq DESC LIMIT 1",
		)
		.bind(project_id)
		.bind(object_digest)
		.fetch_optional(&self.pool)
		.await?;
		Ok(row.map(|row| FeedRow {
			project_id: row.get("project_id"),
			seq: row.get("seq"),
			previous: row.get("previous"),
			entry_digest: row.get("entry_digest"),
			kind: row.get("kind"),
			object_digest: row.get("object_digest"),
			payload: row.get("payload"),
			wire: row.get("wire"),
		}))
	}

	pub async fn objects_of_kind(&self, kind: &str, limit: i64) -> Result<Vec<StoredObject>, sqlx::Error> {
		let rows = sqlx::query("SELECT digest, kind, payload, wire FROM objects WHERE kind = $1 LIMIT $2")
			.bind(kind)
			.bind(limit)
			.fetch_all(&self.pool)
			.await?;
		Ok(rows
			.into_iter()
			.map(|row| StoredObject {
				digest: row.get("digest"),
				kind: row.get("kind"),
				payload: row.get("payload"),
				wire: row.get("wire"),
			})
			.collect())
	}

	pub async fn index_project_delegation(&self, project_id: &str, digest: &[u8]) -> Result<(), sqlx::Error> {
		sqlx::query("INSERT INTO project_delegations (project_id, digest) VALUES ($1, $2) ON CONFLICT DO NOTHING")
			.bind(project_id)
			.bind(digest)
			.execute(&self.pool)
			.await?;
		Ok(())
	}

	pub async fn project_delegations(&self, project_id: &str, limit: i64) -> Result<Vec<StoredObject>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT o.digest, o.kind, o.payload, o.wire FROM objects o
			 JOIN project_delegations d ON d.digest = o.digest WHERE d.project_id = $1 LIMIT $2",
		)
		.bind(project_id)
		.bind(limit)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| StoredObject {
				digest: row.get("digest"),
				kind: row.get("kind"),
				payload: row.get("payload"),
				wire: row.get("wire"),
			})
			.collect())
	}
}
