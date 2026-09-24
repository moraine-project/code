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

#[test]
fn rejects_a_container_length_larger_than_the_input() {
	assert_eq!(
		decode(&[0x9b, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]),
		Err(CodecError::UnexpectedEof),
		"an array cannot claim more elements than there are bytes left"
	);
	assert_eq!(
		decode(&[0xbb, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]),
		Err(CodecError::UnexpectedEof),
		"a map cannot claim more pairs than there are bytes left"
	);
}

#[test]
fn round_trips_the_whole_signed_integer_range() {
	for number in [
		0,
		1,
		-1,
		23,
		24,
		255,
		256,
		-24,
		-25,
		-256,
		-257,
		65_535,
		65_536,
		-65_536,
		-65_537,
		i64::MAX - 1,
		i64::MAX,
		-i64::MAX,
		i64::MIN,
	] {
		let bytes = encode(&Value::Integer(number)).expect("encode");
		assert_eq!(
			decode(&bytes).expect("decode"),
			Value::Integer(number),
			"{number} did not survive"
		);
	}
}

#[test]
fn rejects_a_non_canonical_integer_width() {
	assert_eq!(decode(&[0x18, 0x01]), Err(CodecError::NonMinimalInteger));
	assert_eq!(decode(&[0x38, 0x01]), Err(CodecError::NonMinimalInteger));
}

#[test]
fn accepts_a_large_but_well_formed_array() {
	let mut bytes = vec![0x9a, 0x00, 0x01, 0x00, 0x00];
	bytes.extend(std::iter::repeat_n(0x00, 0x1_0000));
	let value = decode(&bytes).expect("decode");
	let Value::Array(values) = value else {
		panic!("expected an array");
	};
	assert_eq!(values.len(), 0x1_0000);
}
