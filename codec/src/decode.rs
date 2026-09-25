use unicode_normalization::UnicodeNormalization;

use crate::{CodecError, Value};

const MAJOR_UNSIGNED: u8 = 0;
const MAJOR_NEGATIVE: u8 = 1;
const MAJOR_BYTES: u8 = 2;
const MAJOR_TEXT: u8 = 3;
const MAJOR_ARRAY: u8 = 4;
const MAJOR_MAP: u8 = 5;
const MAJOR_TAG: u8 = 6;
const MAJOR_SIMPLE: u8 = 7;

const MAX_DEPTH: usize = 64;

const MAX_PREALLOC: usize = 1024;

pub fn decode(bytes: &[u8]) -> Result<Value, CodecError> {
	let mut parser = Parser {
		input: bytes,
		position: 0,
	};
	let value = parser.parse_value(0)?;
	if parser.position != bytes.len() {
		return Err(CodecError::TrailingBytes);
	}
	Ok(value)
}

struct Parser<'a> {
	input: &'a [u8],
	position: usize,
}

impl Parser<'_> {
	fn parse_value(&mut self, depth: usize) -> Result<Value, CodecError> {
		if depth > MAX_DEPTH {
			return Err(CodecError::DepthLimitExceeded);
		}
		let initial = self.read_byte()?;
		let major = initial >> 5;
		let additional = initial & 0x1f;
		match major {
			MAJOR_UNSIGNED => {
				let argument = self.read_argument(additional)?;
				if argument > i64::MAX as u64 {
					return Err(CodecError::IntegerOutOfRange);
				}
				Ok(Value::Integer(argument as i64))
			}
			MAJOR_NEGATIVE => {
				let argument = self.read_argument(additional)?;
				if argument > 1_u64 << 63 {
					return Err(CodecError::IntegerOutOfRange);
				}
				Ok(Value::Integer(if argument == 1_u64 << 63 {
					i64::MIN
				} else {
					-1 - argument as i64
				}))
			}
			MAJOR_BYTES => {
				let length = self.read_length(additional)?;
				Ok(Value::Bytes(self.read_exact(length)?))
			}
			MAJOR_TEXT => {
				let length = self.read_length(additional)?;
				let raw = self.read_exact(length)?;
				let text = String::from_utf8(raw).map_err(|_| CodecError::InvalidUtf8)?;
				Ok(Value::Text(text))
			}
			MAJOR_ARRAY => {
				let length = self.read_length(additional)?;
				self.check_container_length(length)?;
				let mut values = Vec::with_capacity(length.min(MAX_PREALLOC));
				for _ in 0..length {
					values.push(self.parse_value(depth + 1)?);
				}
				Ok(Value::Array(values))
			}
			MAJOR_MAP => self.parse_map(additional, depth),
			MAJOR_TAG => Err(CodecError::TagForbidden),
			MAJOR_SIMPLE => self.parse_simple(additional),
			_ => unreachable!("major type is a 3-bit value"),
		}
	}

	fn parse_map(&mut self, additional: u8, depth: usize) -> Result<Value, CodecError> {
		let length = self.read_length(additional)?;
		self.check_container_length(length)?;
		let mut pairs = Vec::with_capacity(length.min(MAX_PREALLOC));
		let mut previous: Option<Vec<u8>> = None;
		let mut normalized_keys = std::collections::HashSet::new();
		for _ in 0..length {
			let key_start = self.position;
			let key = self.parse_value(depth + 1)?;
			let key_bytes = self.input[key_start..self.position].to_vec();
			if let Some(previous) = &previous {
				if *previous == key_bytes {
					return Err(CodecError::DuplicateKey);
				}
				if !is_sorted_before(previous, &key_bytes) {
					return Err(CodecError::UnsortedMapKeys);
				}
			}
			if let Value::Text(text) = &key {
				let normalized: String = text.nfc().collect();
				if !normalized_keys.insert(normalized) {
					return Err(CodecError::NormalizationKeyCollision);
				}
			}
			previous = Some(key_bytes);
			let value = self.parse_value(depth + 1)?;
			pairs.push((key, value));
		}
		Ok(Value::Map(pairs))
	}

	fn parse_simple(&mut self, additional: u8) -> Result<Value, CodecError> {
		match additional {
			20 => Ok(Value::Bool(false)),
			21 => Ok(Value::Bool(true)),
			22 => Ok(Value::Null),
			25..=27 => Err(CodecError::FloatForbidden),
			24 => Err(CodecError::UnsupportedSimple),
			31 => Err(CodecError::IndefiniteLengthForbidden),
			_ => Err(CodecError::UnsupportedSimple),
		}
	}

	fn read_argument(&mut self, additional: u8) -> Result<u64, CodecError> {
		match additional {
			0..=23 => Ok(u64::from(additional)),
			24 => {
				let value = u64::from(self.read_byte()?);
				if value < 24 {
					return Err(CodecError::NonMinimalInteger);
				}
				Ok(value)
			}
			25 => {
				let value = u64::from(self.read_u16()?);
				if value <= u64::from(u8::MAX) {
					return Err(CodecError::NonMinimalInteger);
				}
				Ok(value)
			}
			26 => {
				let value = u64::from(self.read_u32()?);
				if value <= u64::from(u16::MAX) {
					return Err(CodecError::NonMinimalInteger);
				}
				Ok(value)
			}
			27 => {
				let value = self.read_u64()?;
				if value <= u64::from(u32::MAX) {
					return Err(CodecError::NonMinimalInteger);
				}
				Ok(value)
			}
			31 => Err(CodecError::IndefiniteLengthForbidden),
			_ => Err(CodecError::UnsupportedSimple),
		}
	}

	fn check_container_length(&self, length: usize) -> Result<(), CodecError> {
		if length > self.input.len().saturating_sub(self.position) {
			return Err(CodecError::UnexpectedEof);
		}
		Ok(())
	}

	fn read_length(&mut self, additional: u8) -> Result<usize, CodecError> {
		if additional <= 23 {
			return Ok(usize::from(additional));
		}
		let value = match additional {
			24 => {
				let value = u64::from(self.read_byte()?);
				if value < 24 {
					return Err(CodecError::NonMinimalLength);
				}
				value
			}
			25 => {
				let value = u64::from(self.read_u16()?);
				if value <= u64::from(u8::MAX) {
					return Err(CodecError::NonMinimalLength);
				}
				value
			}
			26 => {
				let value = u64::from(self.read_u32()?);
				if value <= u64::from(u16::MAX) {
					return Err(CodecError::NonMinimalLength);
				}
				value
			}
			27 => {
				let value = self.read_u64()?;
				if value <= u64::from(u32::MAX) {
					return Err(CodecError::NonMinimalLength);
				}
				value
			}
			31 => return Err(CodecError::IndefiniteLengthForbidden),
			_ => return Err(CodecError::UnsupportedSimple),
		};
		usize::try_from(value).map_err(|_| CodecError::IntegerOutOfRange)
	}

	fn read_byte(&mut self) -> Result<u8, CodecError> {
		let byte = *self.input.get(self.position).ok_or(CodecError::UnexpectedEof)?;
		self.position += 1;
		Ok(byte)
	}

	fn read_exact(&mut self, length: usize) -> Result<Vec<u8>, CodecError> {
		let end = self.position.checked_add(length).ok_or(CodecError::UnexpectedEof)?;
		let slice = self.input.get(self.position..end).ok_or(CodecError::UnexpectedEof)?;
		self.position = end;
		Ok(slice.to_vec())
	}

	fn read_u16(&mut self) -> Result<u16, CodecError> {
		let bytes: [u8; 2] = self.read_exact(2)?.try_into().expect("read_exact returned 2 bytes");
		Ok(u16::from_be_bytes(bytes))
	}

	fn read_u32(&mut self) -> Result<u32, CodecError> {
		let bytes: [u8; 4] = self.read_exact(4)?.try_into().expect("read_exact returned 4 bytes");
		Ok(u32::from_be_bytes(bytes))
	}

	fn read_u64(&mut self) -> Result<u64, CodecError> {
		let bytes: [u8; 8] = self.read_exact(8)?.try_into().expect("read_exact returned 8 bytes");
		Ok(u64::from_be_bytes(bytes))
	}
}

fn is_sorted_before(previous: &[u8], next: &[u8]) -> bool {
	previous.len() < next.len() || (previous.len() == next.len() && previous < next)
}
