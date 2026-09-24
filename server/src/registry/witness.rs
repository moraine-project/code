use std::collections::BTreeMap;

use sqlx::Row;

use crate::db::MetadataStore;

pub struct WitnessObservationRow {
	pub observer_id: String,
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

	pub async fn record_witness_exchange(
		&self,
		observer_id: &str,
		project_id: &str,
		source_home: &str,
		sequence: i64,
		head_entry: &str,
		observed_at: i64,
	) -> Result<(), sqlx::Error> {
		sqlx::query(
			"INSERT INTO witness_exchanges (project_id, observer_id, source_home, sequence, head_entry, observed_at)
			 VALUES ($1, $2, $3, $4, $5, $6)
			 ON CONFLICT(project_id, observer_id, source_home, sequence, head_entry) DO NOTHING",
		)
		.bind(project_id)
		.bind(observer_id)
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
		let mut observations = rows
			.into_iter()
			.map(|row| WitnessObservationRow {
				observer_id: "local".to_string(),
				source_home: row.get("source_home"),
				sequence: row.get("sequence"),
				head_entry: row.get("head_entry"),
				observed_at: row.get("observed_at"),
			})
			.collect::<Vec<_>>();
		let rows = sqlx::query(
			"SELECT observer_id, source_home, sequence, head_entry, observed_at FROM witness_exchanges
			 WHERE project_id = $1 ORDER BY sequence, source_home, observed_at",
		)
		.bind(project_id)
		.fetch_all(&self.pool)
		.await?;
		observations.extend(rows.into_iter().map(|row| WitnessObservationRow {
			observer_id: row.get("observer_id"),
			source_home: row.get("source_home"),
			sequence: row.get("sequence"),
			head_entry: row.get("head_entry"),
			observed_at: row.get("observed_at"),
		}));
		Ok(observations)
	}
}

pub struct WitnessConflict {
	pub source_home: String,
	pub sequence: i64,
	pub entries: Vec<String>,
	pub observers: Vec<String>,
}

pub fn witness_conflicts(observations: &[WitnessObservationRow]) -> Vec<WitnessConflict> {
	let mut grouped: BTreeMap<(i64, &str), Vec<&WitnessObservationRow>> = BTreeMap::new();
	for observation in observations {
		grouped
			.entry((observation.sequence, observation.source_home.as_str()))
			.or_default()
			.push(observation);
	}
	grouped
		.into_iter()
		.filter_map(|((sequence, source_home), rows)| {
			let mut entries: Vec<String> = Vec::new();
			let mut observers: Vec<String> = Vec::new();
			for row in rows {
				if !entries.contains(&row.head_entry) {
					entries.push(row.head_entry.clone());
				}
				if !observers.contains(&row.observer_id) {
					observers.push(row.observer_id.clone());
				}
			}
			(entries.len() > 1).then_some(WitnessConflict {
				source_home: source_home.to_string(),
				sequence,
				entries,
				observers,
			})
		})
		.collect()
}
