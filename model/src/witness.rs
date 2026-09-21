use moraine_codec::Value;

use crate::canonical::{Canonical, Fields, expect_array, expect_i64, expect_text, expect_u32, expect_u64, map_of};
use crate::error::{ModelError, RejectReason};

pub const WITNESS_DOMAIN: &[u8] = b"GAMEDIST/v1/witness\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessObservation {
	pub project_id: String,
	pub source_home: String,
	pub sequence: u64,
	pub head_entry: String,
	pub observed_at: i64,
}

impl Canonical for WitnessObservation {
	fn to_value(&self) -> Value {
		map_of(
			"WitnessObservation",
			[
				("project_id", Value::text(self.project_id.clone())),
				("source_home", Value::text(self.source_home.clone())),
				("sequence", Value::int(self.sequence as i64)),
				("head_entry", Value::text(self.head_entry.clone())),
				("observed_at", Value::int(self.observed_at)),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("WitnessObservation", value)?.reject_unknown(&[
			"project_id",
			"source_home",
			"sequence",
			"head_entry",
			"observed_at",
		])?;
		let observation = Self {
			project_id: expect_text(fields.required("project_id")?, "project_id")?,
			source_home: expect_text(fields.required("source_home")?, "source_home")?,
			sequence: expect_u64(fields.required("sequence")?, "sequence")?,
			head_entry: expect_text(fields.required("head_entry")?, "head_entry")?,
			observed_at: expect_i64(fields.required("observed_at")?, "observed_at")?,
		};
		observation.validate()?;
		Ok(observation)
	}
}

impl WitnessObservation {
	fn validate(&self) -> Result<(), ModelError> {
		if self.project_id.is_empty() || self.source_home.is_empty() || self.head_entry.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "observation"));
		}
		if self.project_id.len() > 128
			|| self.source_home.len() > 2048
			|| self.head_entry.len() > 128
			|| self.sequence > i64::MAX as u64
		{
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "observation"));
		}
		Ok(())
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessBundle {
	pub protocol: u32,
	pub observer_id: String,
	pub observations: Vec<WitnessObservation>,
}

impl Canonical for WitnessBundle {
	fn to_value(&self) -> Value {
		map_of(
			"WitnessBundle",
			[
				("protocol", Value::int(i64::from(self.protocol))),
				("observer_id", Value::text(self.observer_id.clone())),
				(
					"observations",
					Value::array(self.observations.iter().map(Canonical::to_value)),
				),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("WitnessBundle", value)?.reject_unknown(&["protocol", "observer_id", "observations"])?;
		let bundle = Self {
			protocol: expect_u32(fields.required("protocol")?, "protocol")?,
			observer_id: expect_text(fields.required("observer_id")?, "observer_id")?,
			observations: expect_array(fields.required("observations")?, "observations")?
				.iter()
				.cloned()
				.map(WitnessObservation::from_value)
				.collect::<Result<Vec<_>, _>>()?,
		};
		bundle.validate()?;
		Ok(bundle)
	}
}

impl WitnessBundle {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.protocol != 1
			|| self.observer_id.is_empty()
			|| self.observer_id.len() > 128
			|| self.observations.len() > 1_000
		{
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "bundle"));
		}
		for observation in &self.observations {
			observation.validate()?;
		}
		Ok(())
	}
}
