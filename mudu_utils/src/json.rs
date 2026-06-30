use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::json::{from_json_str, to_json_str};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::Path;

pub use mudu::utils::json::{JsonArray, JsonMap, JsonNumber, JsonValue};

pub fn read_json<D: DeserializeOwned, P: AsRef<Path>>(path: P) -> RS<D> {
    let s = mudu_sys::fs::sync::read_to_string(path.as_ref())?;
    let ret: D = from_json_str::<D>(&s)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode from json string error", e))?;
    Ok(ret)
}

pub fn write_json<S: Serialize, P: AsRef<Path>>(object: &S, path: P) -> RS<()> {
    let json_string = to_json_str(object)?;
    mudu_sys::fs::sync::write(path.as_ref(), json_string)?;
    Ok(())
}

#[macro_export]
macro_rules! json_value {
    ($($json:tt)+) => {
        serde_json::json!($($json)+)
    };
}

#[cfg(test)]
mod tests {
    use super::{read_json, write_json};
    use mudu::error::ErrorCode;
    use mudu::utils::json::{JsonValue, from_json_str, to_json_str};
    use serde::{Deserialize, Serialize};
    use std::time::UNIX_EPOCH;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct DemoJson {
        id: u32,
        name: String,
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let suffix = mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        mudu_sys::env_var::temp_dir().join(format!("mudu_json_{name}_{suffix}.json"))
    }

    #[test]
    fn json_string_value_and_file_roundtrip() {
        let value = DemoJson {
            id: 9,
            name: "neo".to_string(),
        };

        let json = to_json_str(&value).unwrap();
        assert!(json.contains("\"name\""));
        let decoded: DemoJson = from_json_str(&json).unwrap();
        assert_eq!(decoded, value);

        let json_value = mudu::utils::json::to_json_value(&value).unwrap();
        let decoded_from_value: DemoJson = mudu::utils::json::from_json_value(json_value).unwrap();
        assert_eq!(decoded_from_value, value);

        let path = temp_path("roundtrip");
        write_json(&value, &path).unwrap();
        let loaded: DemoJson = read_json(&path).unwrap();
        assert_eq!(loaded, value);
    }

    #[test]
    fn json_decode_rejects_wrong_shape() {
        let err =
            mudu::utils::json::from_json_value::<DemoJson>(JsonValue::String("oops".to_string()))
                .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    #[test]
    fn read_json_rejects_missing_file() {
        let path = temp_path("missing");
        let err = read_json::<DemoJson, _>(&path).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
    }

    #[test]
    fn read_json_rejects_malformed_json() {
        let path = temp_path("malformed");
        mudu_sys::fs::sync::write(&path, b"not valid json").unwrap();
        let err = read_json::<DemoJson, _>(&path).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }
}
