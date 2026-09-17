#[cfg(test)]
mod tests {
    use crate::error::ErrorCode;
    use crate::json_value;
    use crate::utils::json::{from_json_str, from_json_value, to_json_str, to_json_value};
    use serde::{Deserialize, Serialize, Serializer};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Sample {
        name: String,
        count: i32,
    }

    #[test]
    fn to_json_str_pretty_prints() {
        let value = json_value!({"ok": true});
        let s = to_json_str(&value).unwrap();
        assert!(s.contains("\"ok\""));
        assert!(s.contains("true"));
    }

    #[test]
    fn from_json_str_roundtrips() {
        let sample = Sample {
            name: "test".to_string(),
            count: 7,
        };
        let json = to_json_str(&sample).unwrap();
        let decoded: Sample = from_json_str(&json).unwrap();
        assert_eq!(decoded, sample);
    }

    #[test]
    fn from_json_str_rejects_invalid_json() {
        let err = from_json_str::<Sample>("not json").unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    #[test]
    fn to_json_value_roundtrips_through_from_json_value() {
        let sample = Sample {
            name: "value".to_string(),
            count: 123,
        };
        let value = to_json_value(&sample).unwrap();
        let decoded: Sample = from_json_value(value).unwrap();
        assert_eq!(decoded, sample);
    }

    #[test]
    fn json_value_macro_builds_value() {
        let value = json_value!({"nested": [1, 2, 3]});
        assert_eq!(value["nested"][1], 2);
    }

    struct AlwaysFails;

    impl Serialize for AlwaysFails {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Err(serde::ser::Error::custom("always fails"))
        }
    }

    #[test]
    fn to_json_str_reports_encoding_errors() {
        let err = to_json_str(&AlwaysFails).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Encode);
        assert!(err.message().contains("encoding json"));
    }

    #[test]
    fn to_json_value_reports_encoding_errors() {
        let err = to_json_value(&AlwaysFails).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Encode);
    }

    #[test]
    fn from_json_value_reports_decoding_errors() {
        let value = json_value!({"unexpected": "shape"});
        let err = from_json_value::<Sample>(value).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }
}
