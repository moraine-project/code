use moraine_codec::Value;

use crate::canonical::{
	Canonical, Fields, canonical_i64, expect_bytes, expect_canonical_u64, expect_i64, expect_text, map_of,
};
use crate::error::{ModelError, RejectReason};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRef {
	pub digest: Vec<u8>,
	pub size: u64,
	pub media_type: String,
}

impl ArtifactRef {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.digest.len() != 32 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "digest"));
		}
		expect_canonical_u64(self.size, "size")?;
		Ok(())
	}
}

impl Canonical for ArtifactRef {
	fn to_value(&self) -> Value {
		map_of(
			"ArtifactRef",
			[
				("digest", Value::bytes(self.digest.clone())),
				("size", Value::int(canonical_i64(self.size, "size"))),
				("media_type", Value::text(self.media_type.clone())),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("ArtifactRef", value)?.reject_unknown(&["digest", "size", "media_type"])?;
		let digest = expect_bytes(fields.required("digest")?, "digest")?;
		let reference = Self {
			digest,
			size: u64::try_from(expect_i64(fields.required("size")?, "size")?)
				.map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, "size"))?,
			media_type: expect_text(fields.required("media_type")?, "media_type")?,
		};
		reference.validate()?;
		Ok(reference)
	}
}
