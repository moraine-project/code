use unicode_normalization::UnicodeNormalization;

use crate::{CodecError, Value};

const MAJOR_UNSIGNED: u8 = 0;
const MAJOR_NEGATIVE: u8 = 1;
const MAJOR_BYTES: u8 = 2;
const MAJOR_TEXT: u8 = 3;
const MAJOR_ARRAY: u8 = 4;
const MAJOR_MAP: u8 = 5;
const MAJOR_SIMPLE: u8 = 7;

const SIMPLE_FALSE: u8 = 20;
const SIMPLE_TRUE: u8 = 21;
const SIMPLE_NULL: u8 = 22;

const MAX_DEPTH: usize = 64;

pub fn encode(value: &Value) -> Result<Vec<u8>, CodecError> {
	let mut out = Vec::new();
	encode_value(value, &mut out, 0)?;
	Ok(out)
}

pub(crate) fn encode_value(value: &Value, out: &mut Vec<u8>, depth: usize) -> Result<(), CodecError> {
	if depth > MAX_DEPTH {
		return Err(CodecError::DepthLimitExceeded);
	}
	match value {
		Value::Integer(number) => {
			if *number >= 0 {
				write_head(out, MAJOR_UNSIGNED, *number as u64);
			} else {
				let magnitude = (-1i128 - i128::from(*number)) as u64;
				write_head(out, MAJOR_NEGATIVE, magnitude);
			}
		}
		Value::Bytes(bytes) => {
			write_head(out, MAJOR_BYTES, bytes.len() as u64);
			out.extend_from_slice(bytes);
		}
		Value::Text(text) => {
			write_head(out, MAJOR_TEXT, text.len() as u64);
			out.extend_from_slice(text.as_bytes());
		}
		Value::Bool(false) => write_head(out, MAJOR_SIMPLE, SIMPLE_FALSE as u64),
		Value::Bool(true) => write_head(out, MAJOR_SIMPLE, SIMPLE_TRUE as u64),
		Value::Null => write_head(out, MAJOR_SIMPLE, SIMPLE_NULL as u64),
		Value::Array(values) => {
			write_head(out, MAJOR_ARRAY, values.len() as u64);
			for value in values {
				encode_value(value, out, depth + 1)?;
			}
		}
		Value::Map(pairs) => encode_map(pairs, out, depth)?,
	}
	Ok(())
}

fn encode_map(pairs: &[(Value, Value)], out: &mut Vec<u8>, depth: usize) -> Result<(), CodecError> {
	let mut encoded = Vec::with_capacity(pairs.len());
	for (key, value) in pairs {
		let mut key_bytes = Vec::new();
		encode_value(key, &mut key_bytes, depth + 1)?;
		let mut value_bytes = Vec::new();
		encode_value(value, &mut value_bytes, depth + 1)?;
		encoded.push((key_bytes, value_bytes, key));
	}
	encoded.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0)));

	let mut normalized_keys = std::collections::HashSet::new();
	for window in encoded.windows(2) {
		if window[0].0 == window[1].0 {
			return Err(CodecError::DuplicateKey);
		}
	}
	for (_, _, key) in &encoded {
		if let Value::Text(text) = key {
			let normalized: String = text.nfc().collect();
			if !normalized_keys.insert(normalized) {
				return Err(CodecError::NormalizationKeyCollision);
			}
		}
	}

	write_head(out, MAJOR_MAP, pairs.len() as u64);
	for (key_bytes, value_bytes, _) in encoded {
		out.extend_from_slice(&key_bytes);
		out.extend_from_slice(&value_bytes);
	}
	Ok(())
}

pub(crate) fn write_head(out: &mut Vec<u8>, major: u8, argument: u64) {
	let major = major << 5;
	if argument < 24 {
		out.push(major | argument as u8);
	} else if argument <= u64::from(u8::MAX) {
		out.push(major | 24);
		out.push(argument as u8);
	} else if argument <= u64::from(u16::MAX) {
		out.push(major | 25);
		out.extend_from_slice(&(argument as u16).to_be_bytes());
	} else if argument <= u64::from(u32::MAX) {
		out.push(major | 26);
		out.extend_from_slice(&(argument as u32).to_be_bytes());
	} else {
		out.push(major | 27);
		out.extend_from_slice(&argument.to_be_bytes());
	}
}
