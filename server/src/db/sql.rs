use sqlx::AssertSqlSafe;
use sqlx::any::{Any, AnyArguments};
use sqlx::query::Query;

#[derive(Debug, Clone)]
pub enum Value {
	Int(i64),
	Text(String),
	Bytes(Vec<u8>),
}

impl From<i64> for Value {
	fn from(value: i64) -> Self {
		Self::Int(value)
	}
}

impl From<&str> for Value {
	fn from(value: &str) -> Self {
		Self::Text(value.to_string())
	}
}

impl From<String> for Value {
	fn from(value: String) -> Self {
		Self::Text(value)
	}
}

impl From<&String> for Value {
	fn from(value: &String) -> Self {
		Self::Text(value.clone())
	}
}

impl From<Vec<u8>> for Value {
	fn from(value: Vec<u8>) -> Self {
		Self::Bytes(value)
	}
}

impl From<&[u8]> for Value {
	fn from(value: &[u8]) -> Self {
		Self::Bytes(value.to_vec())
	}
}

pub struct SqlBuilder {
	text: String,
	values: Vec<Value>,
}

impl SqlBuilder {
	pub fn new(prefix: &str) -> Self {
		Self {
			text: prefix.to_string(),
			values: Vec::new(),
		}
	}

	pub fn push(&mut self, sql: &str) -> &mut Self {
		self.text.push_str(sql);
		self
	}

	pub fn push_bind(&mut self, value: impl Into<Value>) -> &mut Self {
		self.bind(value)
	}

	fn bind(&mut self, value: impl Into<Value>) -> &mut Self {
		self.values.push(value.into());
		self.text.push_str(&format!("${}", self.values.len()));
		self
	}

	pub fn reserve_bind(&mut self, value: impl Into<Value>) -> usize {
		self.values.push(value.into());
		self.values.len()
	}

	pub fn separated(&mut self, separator: &str) -> Separated<'_> {
		Separated {
			builder: self,
			separator: separator.to_string(),
			started: false,
		}
	}

	pub fn into_query(self) -> Query<'static, Any, AnyArguments> {
		let mut query = sqlx::query(AssertSqlSafe(self.text));
		for value in self.values {
			query = match value {
				Value::Int(value) => query.bind(value),
				Value::Text(value) => query.bind(value),
				Value::Bytes(value) => query.bind(value),
			};
		}
		query
	}
}

pub struct Separated<'a> {
	builder: &'a mut SqlBuilder,
	separator: String,
	started: bool,
}

impl Separated<'_> {
	fn separate(&mut self) {
		if self.started {
			self.builder.text.push_str(&self.separator);
		}
		self.started = true;
	}

	pub fn push(&mut self, sql: &str) -> &mut Self {
		self.separate();
		self.builder.text.push_str(sql);
		self
	}

	pub fn push_bind(&mut self, value: impl Into<Value>) -> &mut Self {
		self.separate();
		self.builder.bind(value);
		self
	}

	pub fn push_unseparated(&mut self, sql: &str) -> &mut Self {
		self.builder.text.push_str(sql);
		self
	}

	pub fn push_bind_unseparated(&mut self, value: impl Into<Value>) -> &mut Self {
		self.builder.bind(value);
		self
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn numbers_placeholders_in_order() {
		let mut builder = SqlBuilder::new("SELECT a FROM t WHERE b = ");
		builder
			.push_bind(1i64)
			.push(" AND c IN (")
			.push_bind("x")
			.push(") LIMIT ")
			.push_bind(2i64);

		assert_eq!(builder.text, "SELECT a FROM t WHERE b = $1 AND c IN ($2) LIMIT $3");
	}

	#[test]
	fn builds_tuple_lists_with_an_inner_separator() {
		let mut builder = SqlBuilder::new("SELECT a FROM t WHERE (b, c) IN (");
		{
			let mut separated = builder.separated(", ");
			for (b, c) in [(1i64, "x"), (2i64, "y")] {
				separated
					.push("(")
					.push_bind_unseparated(b)
					.push_unseparated(", ")
					.push_bind_unseparated(c)
					.push_unseparated(")");
			}
		}
		builder.push(")");

		assert_eq!(builder.text, "SELECT a FROM t WHERE (b, c) IN (($1, $2), ($3, $4))");
	}

	#[test]
	fn separates_list_entries_without_a_leading_separator() {
		let mut builder = SqlBuilder::new("SELECT a FROM t WHERE id IN (");
		{
			let mut separated = builder.separated(", ");
			for id in ["a", "b", "c"] {
				separated.push_bind(id);
			}
		}
		builder.push(")");

		assert_eq!(builder.text, "SELECT a FROM t WHERE id IN ($1, $2, $3)");
	}
}
