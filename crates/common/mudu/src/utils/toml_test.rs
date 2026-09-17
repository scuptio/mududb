#[cfg(test)]
mod tests {
    use crate::error::ErrorCode;
    use crate::utils::toml::to_toml_str;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Sample {
        name: String,
        count: u32,
    }

    #[test]
    fn to_toml_str_serializes_valid_object() {
        let sample = Sample {
            name: "example".to_string(),
            count: 7,
        };
        let s = to_toml_str(&sample).unwrap();
        assert!(s.contains("name = \"example\""));
        assert!(s.contains("count = 7"));
    }

    #[test]
    fn to_toml_str_reports_encoding_errors() {
        // A map with tuple keys cannot be serialized to TOML.
        let mut bad = std::collections::BTreeMap::new();
        bad.insert(("a".to_string(), "b".to_string()), "value".to_string());
        let err = to_toml_str(&bad).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Encode);
    }
}
