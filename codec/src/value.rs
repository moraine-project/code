#[derive(Debug, Clone, Eq)]
pub enum Value {
	Integer(i64),
	Bytes(Vec<u8>),
	Text(String),
	Bool(bool),
	Null,
	Array(Vec<Value>),
	Map(Vec<(Value, Value)>),
}

impl PartialEq for Value {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::Integer(a), Self::Integer(b)) => a == b,
			(Self::Bytes(a), Self::Bytes(b)) => a == b,
			(Self::Text(a), Self::Text(b)) => a == b,
			(Self::Bool(a), Self::Bool(b)) => a == b,
			(Self::Null, Self::Null) => true,
			(Self::Array(a), Self::Array(b)) => a == b,
			(Self::Map(a), Self::Map(b)) => maps_equal(a, b),
			_ => false,
		}
	}
}

fn maps_equal(a: &[(Value, Value)], b: &[(Value, Value)]) -> bool {
	if a.len() != b.len() {
		return false;
	}
	a.iter().all(|(key, value)| {
		b.iter()
			.any(|(other_key, other_value)| key == other_key && value == other_value)
	})
}

impl Value {
	pub fn int(value: i64) -> Self {
		Self::Integer(value)
	}

	pub fn bytes(value: impl Into<Vec<u8>>) -> Self {
		Self::Bytes(value.into())
	}

	pub fn text(value: impl Into<String>) -> Self {
		Self::Text(value.into())
	}

	pub fn array(values: impl IntoIterator<Item = Value>) -> Self {
		Self::Array(values.into_iter().collect())
	}

	pub fn map(pairs: impl IntoIterator<Item = (Value, Value)>) -> Self {
		Self::Map(pairs.into_iter().collect())
	}

	pub fn as_int(&self) -> Option<i64> {
		match self {
			Self::Integer(value) => Some(*value),
			_ => None,
		}
	}

	pub fn as_bytes(&self) -> Option<&[u8]> {
		match self {
			Self::Bytes(value) => Some(value),
			_ => None,
		}
	}

	pub fn as_text(&self) -> Option<&str> {
		match self {
			Self::Text(value) => Some(value),
			_ => None,
		}
	}

	pub fn as_bool(&self) -> Option<bool> {
		match self {
			Self::Bool(value) => Some(*value),
			_ => None,
		}
	}

	pub fn as_array(&self) -> Option<&[Value]> {
		match self {
			Self::Array(values) => Some(values),
			_ => None,
		}
	}

	pub fn as_map(&self) -> Option<&[(Value, Value)]> {
		match self {
			Self::Map(pairs) => Some(pairs),
			_ => None,
		}
	}

	pub fn get(&self, key: &str) -> Option<&Value> {
		self.as_map()?.iter().find(|(k, _)| k.as_text() == Some(key)).map(|(_, v)| v)
	}
}
