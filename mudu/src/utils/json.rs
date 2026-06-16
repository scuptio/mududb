#![allow(clippy::disallowed_methods)]

use crate::common::result::RS;
use crate::error::ec::EC;
use crate::m_error;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

pub type JsonNumber = serde_json::Number;
pub type JsonValue = serde_json::Value;
pub type JsonMap<K, V> = serde_json::Map<K, V>;
pub type JsonArray = Vec<JsonValue>;

#[macro_export]
macro_rules! json_value {
    // Hide distracting implementation details from the generated rustdoc.
    ($($json:tt)+) => {
        serde_json::json!($($json)+)
    };
}

pub fn to_json_str<S: Serialize>(value: &S) -> RS<String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| m_error!(EC::EncodeErr, "error when encoding json", e))
}

pub fn from_json_str<D: DeserializeOwned>(s: &str) -> RS<D> {
    serde_json::from_str(s).map_err(|e| m_error!(EC::DecodeErr, "error when decoding json", e))
}

pub fn to_json_value<S: Serialize>(value: &S) -> RS<JsonValue> {
    serde_json::to_value(value).map_err(|e| m_error!(EC::EncodeErr, "error when encoding json", e))
}

pub fn from_json_value<D: DeserializeOwned>(s: JsonValue) -> RS<D> {
    serde_json::from_value(s).map_err(|e| m_error!(EC::DecodeErr, "error when decoding json", e))
}

pub fn read_json<D: DeserializeOwned, P: AsRef<Path>>(path: P) -> RS<D> {
    let s = fs::read_to_string(path.as_ref()).map_err(|e| {
        m_error!(
            EC::IOErr,
            format!("read json file {:?} error", path.as_ref()),
            e
        )
    })?;
    let ret: D = from_json_str::<D>(&s)
        .map_err(|e| m_error!(EC::DecodeErr, "decode from toml string error", e))?;
    Ok(ret)
}

pub fn write_json<S: Serialize, P: AsRef<Path>>(object: &S, path: P) -> RS<()> {
    let json_string = to_json_str(object)?;
    fs::write(path.as_ref(), json_string).map_err(|e| {
        m_error!(
            EC::IOErr,
            format!("write json to file {:?} error", path.as_ref()),
            e
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        JsonValue, from_json_str, from_json_value, read_json, to_json_str, to_json_value,
        write_json,
    };
    use serde::{Deserialize, Serialize};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct DemoJson {
        id: u32,
        name: String,
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("mudu_json_{name}_{suffix}.json"))
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

        let json_value = to_json_value(&value).unwrap();
        let decoded_from_value: DemoJson = from_json_value(json_value).unwrap();
        assert_eq!(decoded_from_value, value);

        let path = temp_path("roundtrip");
        write_json(&value, &path).unwrap();
        let loaded: DemoJson = read_json(&path).unwrap();
        assert_eq!(loaded, value);
    }

    #[test]
    fn json_decode_rejects_wrong_shape() {
        let err = from_json_value::<DemoJson>(JsonValue::String("oops".to_string())).unwrap_err();
        assert!(err.to_string().contains("DecodeErr"));
    }
}
