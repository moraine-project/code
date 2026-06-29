use moraine_codec::Value;

use crate::canonical::{Canonical, Fields, expect_bytes, expect_i64, expect_text, map_of};
use crate::error::{ModelError, RejectReason};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRef {
	pub digest: Vec<u8>,
	pub size: u64,
	pub media_type: String,
}

impl Canonical for ArtifactRef {
	fn to_value(&self) -> Value {
		map_of(
			"ArtifactRef",
			[
				("digest", Value::bytes(self.digest.clone())),
				("size", Value::int(self.size as i64)),
				("media_type", Value::text(self.media_type.clone())),
			],
		)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("ArtifactRef", value)?.reject_unknown(&["digest", "size", "media_type"])?;
		let digest = expect_bytes(fields.required("digest")?, "digest")?;
		if digest.len() != 32 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "digest"));
		}
		Ok(Self {
			digest,
			size: u64::try_from(expect_i64(fields.required("size")?, "size")?)
				.map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, "size"))?,
			media_type: expect_text(fields.required("media_type")?, "media_type")?,
		})
	}
}
