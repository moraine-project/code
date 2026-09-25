use moraine_codec::Value;

use crate::canonical::{
	Canonical, Fields, canonical_i64, expect_bool, expect_bytes, expect_canonical_u64, expect_i64, expect_text,
	expect_text_array, map_of,
};
use crate::error::{ModelError, RejectReason};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
	pub digest: Vec<u8>,
	pub size: u64,
	pub media_type: String,
	pub filename: String,
	pub is_primary: bool,
	pub os_predicate: Option<Vec<String>>,
	pub arch_predicate: Option<Vec<String>>,
}

impl Artifact {
	pub fn validate(&self) -> Result<(), ModelError> {
		if self.digest.len() != 32 {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "digest"));
		}
		expect_canonical_u64(self.size, "size")?;
		if self.filename.is_empty() {
			return Err(ModelError::field(RejectReason::InvalidFieldValue, "filename"));
		}
		Ok(())
	}
}

impl Canonical for Artifact {
	fn to_value(&self) -> Value {
		let mut pairs = vec![
			("digest", Value::bytes(self.digest.clone())),
			("size", Value::int(canonical_i64(self.size, "size"))),
			("media_type", Value::text(self.media_type.clone())),
			("filename", Value::text(self.filename.clone())),
			("is_primary", Value::Bool(self.is_primary)),
		];
		if let Some(os) = &self.os_predicate {
			pairs.push((
				"os_predicate",
				Value::array(os.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			));
		}
		if let Some(arch) = &self.arch_predicate {
			pairs.push((
				"arch_predicate",
				Value::array(arch.iter().cloned().map(Value::text).collect::<Vec<_>>()),
			));
		}
		map_of("Artifact", pairs)
	}

	fn from_value(value: Value) -> Result<Self, ModelError> {
		let fields = Fields::new("Artifact", value)?.reject_unknown(&[
			"digest",
			"size",
			"media_type",
			"filename",
			"is_primary",
			"os_predicate",
			"arch_predicate",
		])?;
		let digest = expect_bytes(fields.required("digest")?, "digest")?;
		let artifact = Self {
			digest,
			size: u64::try_from(expect_i64(fields.required("size")?, "size")?)
				.map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, "size"))?,
			media_type: expect_text(fields.required("media_type")?, "media_type")?,
			filename: expect_text(fields.required("filename")?, "filename")?,
			is_primary: expect_bool(fields.required("is_primary")?, "is_primary")?,
			os_predicate: fields
				.optional("os_predicate")
				.map(|v| expect_text_array(v, "os_predicate"))
				.transpose()?,
			arch_predicate: fields
				.optional("arch_predicate")
				.map(|v| expect_text_array(v, "arch_predicate"))
				.transpose()?,
		};
		artifact.validate()?;
		Ok(artifact)
	}
}
