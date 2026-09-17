#[cfg(test)]
mod tests {
    use crate::common::serde_utils::{
        deserialize_from, deserialize_from_json, deserialize_sized_from, header_size_len,
        serialize_sized_to, serialize_sized_to_vec, serialize_to, serialize_to_json,
        serialize_to_vec,
    };
    use crate::error::ErrorCode;
    use serde::ser::Error as SerError;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestValue {
        id: u64,
        name: String,
    }

    fn sample_value() -> TestValue {
        TestValue {
            id: 42,
            name: "mudu".to_string(),
        }
    }

    #[test]
    fn header_size_len_returns_eight() {
        assert_eq!(header_size_len(), 8);
    }

    #[test]
    fn serialize_to_vec_round_trips() {
        let value = sample_value();
        let bytes = serialize_to_vec(&value).unwrap();
        let (decoded, consumed) = deserialize_from::<TestValue>(&bytes).unwrap();
        assert_eq!(decoded, value);
        assert_eq!(consumed, bytes.len() as u64);
    }

    #[test]
    fn serialize_to_buffer_round_trips() {
        let value = sample_value();
        let mut buf = vec![0u8; 64];
        let n = serialize_to(&value, &mut buf).unwrap();
        assert!(n > 0);
        let (decoded, consumed) = deserialize_from::<TestValue>(&buf[..n as usize]).unwrap();
        assert_eq!(decoded, value);
        assert_eq!(consumed, n);
    }

    #[test]
    fn serialize_to_reports_insufficient_buffer() {
        let value = sample_value();
        let mut buf = [0u8; 1];
        let err = serialize_to(&value, &mut buf).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Encode);
    }

    #[test]
    fn serialize_sized_to_vec_round_trips() {
        let value = sample_value();
        let bytes = serialize_sized_to_vec(&value).unwrap();
        let (decoded, size) = deserialize_sized_from::<TestValue>(&bytes).unwrap();
        assert_eq!(decoded, value);
        assert_eq!(size + header_size_len(), bytes.len() as u64);
    }

    #[test]
    fn serialize_sized_to_small_buffer_fails() {
        let value = sample_value();
        let mut buf = [0u8; 4];
        let err = serialize_sized_to(&value, &mut buf).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InsufficientBufferSpace);
    }

    #[test]
    fn serialize_sized_to_exactly_header_size_fails_to_write_body() {
        let value = sample_value();
        let mut buf = vec![0u8; header_size_len() as usize];
        let result = serialize_sized_to(&value, &mut buf).unwrap();
        assert!(!result.0);
        assert!(result.1 > 0);
    }

    #[test]
    fn deserialize_sized_from_rejects_short_buffer_for_length() {
        let err = deserialize_sized_from::<TestValue>(&[0u8; 4]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InsufficientBufferSpace);
    }

    #[test]
    fn deserialize_sized_from_rejects_truncated_body() {
        let mut buf = vec![0u8; 64];
        // length prefix claims 100 bytes but only a few follow
        buf[7] = 100;
        let err = deserialize_sized_from::<TestValue>(&buf).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InsufficientBufferSpace);
    }

    #[test]
    fn deserialize_sized_from_rejects_invalid_messagepack() {
        // 8-byte length prefix with invalid body
        let mut buf = vec![0u8; 16];
        buf[7] = 8;
        let err = deserialize_sized_from::<TestValue>(&buf).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    #[test]
    fn deserialize_from_rejects_invalid_messagepack() {
        let err = deserialize_from::<TestValue>(&[0xc1]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    #[test]
    fn serialize_to_json_round_trips() {
        let value = sample_value();
        let json = serialize_to_json(&value).unwrap();
        let decoded: TestValue = deserialize_from_json(&json).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn deserialize_from_json_rejects_invalid_json() {
        let err = deserialize_from_json::<TestValue>("not json").unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    #[test]
    fn serialize_sized_to_vec_handles_empty_value() {
        let value = String::new();
        let bytes = serialize_sized_to_vec::<String>(&value).unwrap();
        let (decoded, size): (String, u64) = deserialize_sized_from::<String>(&bytes).unwrap();
        assert_eq!(decoded, value);
        assert_eq!(size + header_size_len(), bytes.len() as u64);
    }

    #[test]
    fn serialize_to_vec_handles_nested_structures() {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        struct Inner {
            values: Vec<u32>,
        }

        let value = Inner {
            values: vec![1, 2, 3, 4, 5],
        };
        let bytes = serialize_to_vec(&value).unwrap();
        let (decoded, _) = deserialize_from::<Inner>(&bytes).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn writer_and_sizer_flush_are_no_ops() {
        use crate::common::serde_utils::{Sizer, Writer};
        use std::io::Write;

        let mut buf = [0u8; 16];
        let mut writer = Writer::new(&mut buf);
        assert!(writer.flush().is_ok());

        let mut sizer = Sizer::new();
        assert!(sizer.flush().is_ok());
    }

    #[test]
    fn serialize_sized_to_vec_resizes_when_value_exceeds_initial_buffer() {
        // INIT_LENGTH is 256; a 300-byte payload forces the resize branch.
        let value = vec![0u8; 300];
        let bytes = serialize_sized_to_vec(&value).unwrap();
        let (decoded, size): (Vec<u8>, u64) = deserialize_sized_from::<Vec<u8>>(&bytes).unwrap();
        assert_eq!(decoded, value);
        assert!(size > 0);
    }

    #[test]
    fn debug_sized_helpers_run_with_test_flag() {
        use crate::common::serde_utils::{__deserialize_sized_from, __serialize_sized_to_vec};
        let value = sample_value();
        let bytes = __serialize_sized_to_vec::<TestValue, true>(&value).unwrap();
        let (decoded, _) = __deserialize_sized_from::<TestValue, true>(&bytes).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn serialize_sized_to_reports_size_for_small_buffer() {
        use crate::common::serde_utils::__serialize_sized_to;
        let value = sample_value();
        // Large enough for the 8-byte length header, too small for the body.
        let mut buf = vec![0u8; header_size_len() as usize + 1];
        let (ok, size) = __serialize_sized_to::<TestValue, true>(&value, &mut buf).unwrap();
        assert!(!ok);
        assert!(size > 0);
    }

    #[derive(Debug, Deserialize)]
    struct FailingSerialize;

    impl Serialize for FailingSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(S::Error::custom("intentional serialization failure"))
        }
    }

    #[test]
    fn serialize_sized_to_maps_non_value_write_errors() {
        use crate::common::serde_utils::__serialize_sized_to;
        let value = FailingSerialize;
        let mut buf = vec![0u8; 256];
        let err = __serialize_sized_to::<_, false>(&value, &mut buf).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Encode);
    }
}
