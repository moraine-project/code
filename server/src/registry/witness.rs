use std::collections::BTreeMap;

use sqlx::Row;

use crate::db::MetadataStore;

pub struct WitnessObservationRow {
	pub source_home: String,
	pub sequence: i64,
	pub head_entry: String,
	pub observed_at: i64,
}

impl MetadataStore {
	pub async fn record_witness_observation(
		&self,
		project_id: &str,
		source_home: &str,
		sequence: i64,
		head_entry: &str,
		observed_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO witness_observations (project_id, source_home, sequence, head_entry, observed_at)
			 VALUES ($1, $2, $3, $4, $5)
			 ON CONFLICT(project_id, source_home, sequence, head_entry) DO NOTHING",
		)
		.bind(project_id)
		.bind(source_home)
		.bind(sequence)
		.bind(head_entry)
		.bind(observed_at)
		.execute(&self.pool)
		.await?;
		Ok(())
	}

	pub async fn witness_observations(&self, project_id: &str) -> Result<Vec<WitnessObservationRow>, sqlx::Error> {
		let rows = sqlx::query(
			"SELECT source_home, sequence, head_entry, observed_at FROM witness_observations
			 WHERE project_id = $1 ORDER BY sequence, source_home, observed_at",
		)
		.bind(project_id)
		.fetch_all(&self.pool)
		.await?;
		Ok(rows
			.into_iter()
			.map(|row| WitnessObservationRow {
				source_home: row.get("source_home"),
				sequence: row.get("sequence"),
				head_entry: row.get("head_entry"),
				observed_at: row.get("observed_at"),
			})
			.collect())
	}
}

pub struct WitnessConflict {
	pub sequence: i64,
	pub entries: Vec<String>,
	pub homes: Vec<String>,
}

pub fn witness_conflicts(observations: &[WitnessObservationRow]) -> Vec<WitnessConflict> {
	let mut by_sequence: BTreeMap<i64, Vec<&WitnessObservationRow>> = BTreeMap::new();
	for observation in observations {
		by_sequence.entry(observation.sequence).or_default().push(observation);
	}
	by_sequence
		.into_iter()
		.filter_map(|(sequence, rows)| {
			let mut entries: Vec<String> = Vec::new();
			let mut homes: Vec<String> = Vec::new();
			for row in rows {
				if !entries.contains(&row.head_entry) {
					entries.push(row.head_entry.clone());
				}
				if !homes.contains(&row.source_home) {
					homes.push(row.source_home.clone());
				}
			}
			(entries.len() > 1).then_some(WitnessConflict {
				sequence,
				entries,
				homes,
			})
		})
		.collect()
}
