use moraine_codec::{CodecError, Value, decode, encode};

fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn integers_use_the_minimal_width() {
	assert_eq!(hex(&encode(&Value::int(0)).unwrap()), "00");
	assert_eq!(hex(&encode(&Value::int(23)).unwrap()), "17");
	assert_eq!(hex(&encode(&Value::int(24)).unwrap()), "1818");
	assert_eq!(hex(&encode(&Value::int(255)).unwrap()), "18ff");
	assert_eq!(hex(&encode(&Value::int(256)).unwrap()), "190100");
	assert_eq!(hex(&encode(&Value::int(65536)).unwrap()), "1a00010000");
	assert_eq!(hex(&encode(&Value::int(-1)).unwrap()), "20");
	assert_eq!(hex(&encode(&Value::int(-24)).unwrap()), "37");
	assert_eq!(hex(&encode(&Value::int(-25)).unwrap()), "3818");
}

#[test]
fn map_keys_sort_shortest_first_then_bytewise() {
	let value = Value::map([(Value::text("aa"), Value::int(1)), (Value::text("b"), Value::int(2))]);
	assert_eq!(hex(&encode(&value).unwrap()), "a261620262616101");
}

#[test]
fn rejects_non_minimal_integers() {
	assert_eq!(decode(&[0x18, 0x17]), Err(CodecError::NonMinimalInteger));
	assert_eq!(decode(&[0x19, 0x00, 0xff]), Err(CodecError::NonMinimalInteger));
}

#[test]
fn rejects_non_minimal_lengths() {
	assert_eq!(decode(&[0x58, 0x01, 0x00]), Err(CodecError::NonMinimalLength));
	assert_eq!(decode(&[0x78, 0x01, 0x61]), Err(CodecError::NonMinimalLength));
}

#[test]
fn rejects_duplicate_keys() {
	assert_eq!(
		decode(&[0xa2, 0x61, 0x61, 0x01, 0x61, 0x61, 0x02]),
		Err(CodecError::DuplicateKey)
	);
}

#[test]
fn rejects_out_of_order_map_keys() {
	assert_eq!(
		decode(&[0xa2, 0x61, 0x62, 0x01, 0x61, 0x61, 0x02]),
		Err(CodecError::UnsortedMapKeys)
	);
}

#[test]
fn rejects_indefinite_length() {
	assert_eq!(decode(&[0x9f, 0xff]), Err(CodecError::IndefiniteLengthForbidden));
	assert_eq!(decode(&[0x7f, 0x61, 0xff]), Err(CodecError::IndefiniteLengthForbidden));
}

#[test]
fn rejects_floats_tags_and_trailing_bytes() {
	assert_eq!(decode(&[0xf9, 0x00, 0x00]), Err(CodecError::FloatForbidden));
	assert_eq!(decode(&[0xc0, 0x60]), Err(CodecError::TagForbidden));
	assert_eq!(decode(&[0x00, 0x00]), Err(CodecError::TrailingBytes));
}

#[test]
fn rejects_keys_that_differ_only_by_normalization() {
	let value = Value::map([
		(Value::text("\u{e9}"), Value::int(1)),
		(Value::text("e\u{301}"), Value::int(2)),
	]);
	assert_eq!(encode(&value), Err(CodecError::NormalizationKeyCollision));
}

#[test]
fn round_trips_a_nested_value() {
	let value = Value::map([
		(
			Value::text("array"),
			Value::array([Value::int(1), Value::bytes(vec![1, 2, 3]), Value::Null]),
		),
		(Value::text("nested"), Value::map([(Value::int(7), Value::Bool(true))])),
	]);
	let bytes = encode(&value).unwrap();
	assert_eq!(decode(&bytes).unwrap(), value);
}
