use moraine_codec::{Value, decode, encode};

use crate::error::{ModelError, RejectReason};

pub trait Canonical: Sized {
	fn to_value(&self) -> Value;

	fn from_value(value: Value) -> Result<Self, ModelError>;

	fn to_canonical_bytes(&self) -> Vec<u8> {
		encode(&self.to_value()).expect("model values are always encodable")
	}

	fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ModelError> {
		let value = decode(bytes).map_err(ModelError::from_codec)?;
		Self::from_value(value)
	}
}

pub struct Fields(Vec<(String, Value)>);

impl Fields {
	pub fn new(key: &str, value: Value) -> Result<Self, ModelError> {
		let pairs = value
			.as_map()
			.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldType, key))?
			.iter()
			.map(|(k, v)| {
				let name = k
					.as_text()
					.ok_or_else(|| ModelError::new(RejectReason::InvalidFieldType, "map key is not text"))?;
				Ok((name.to_string(), v.clone()))
			})
			.collect::<Result<Vec<_>, ModelError>>()?;
		Ok(Self(pairs))
	}

	pub fn required(&self, key: &str) -> Result<&Value, ModelError> {
		self.get(key)
			.ok_or_else(|| ModelError::field(RejectReason::MissingField, key))
	}

	pub fn optional(&self, key: &str) -> Option<&Value> {
		self.get(key)
	}

	fn get(&self, key: &str) -> Option<&Value> {
		self.0.iter().find(|(name, _)| name == key).map(|(_, value)| value)
	}

	pub fn reject_unknown(self, allowed: &[&str]) -> Result<Self, ModelError> {
		for (name, _) in &self.0 {
			if !allowed.contains(&name.as_str()) {
				return Err(ModelError::field(RejectReason::UnknownField, name));
			}
		}
		Ok(self)
	}

	pub fn names(&self) -> impl Iterator<Item = &str> {
		self.0.iter().map(|(name, _)| name.as_str())
	}
}

pub fn expect_text(value: &Value, key: &str) -> Result<String, ModelError> {
	value
		.as_text()
		.map(str::to_owned)
		.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldType, key))
}

pub fn expect_bytes(value: &Value, key: &str) -> Result<Vec<u8>, ModelError> {
	value
		.as_bytes()
		.map(<[u8]>::to_vec)
		.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldType, key))
}

pub fn expect_bool(value: &Value, key: &str) -> Result<bool, ModelError> {
	value
		.as_bool()
		.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldType, key))
}

pub fn expect_u64(value: &Value, key: &str) -> Result<u64, ModelError> {
	let number = value
		.as_int()
		.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldType, key))?;
	u64::try_from(number).map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, key))
}

pub fn expect_canonical_u64(value: u64, key: &str) -> Result<u64, ModelError> {
	if value > i64::MAX as u64 {
		return Err(ModelError::field(RejectReason::InvalidFieldValue, key));
	}
	Ok(value)
}

pub fn canonical_i64(value: u64, key: &str) -> i64 {
	i64::try_from(value).unwrap_or_else(|_| panic!("{key} exceeds the canonical integer range"))
}

pub fn expect_u32(value: &Value, key: &str) -> Result<u32, ModelError> {
	let number = expect_u64(value, key)?;
	u32::try_from(number).map_err(|_| ModelError::field(RejectReason::InvalidFieldValue, key))
}

pub fn expect_i64(value: &Value, key: &str) -> Result<i64, ModelError> {
	value
		.as_int()
		.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldType, key))
}

pub fn expect_array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value], ModelError> {
	value
		.as_array()
		.ok_or_else(|| ModelError::field(RejectReason::InvalidFieldType, key))
}

pub fn expect_text_array(value: &Value, key: &str) -> Result<Vec<String>, ModelError> {
	expect_array(value, key)?.iter().map(|item| expect_text(item, key)).collect()
}

pub fn map_of(_label: &str, pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
	Value::map(pairs.into_iter().map(|(name, value)| (Value::text(name), value)))
}
