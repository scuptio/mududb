use crate::common::result::RS;
use crate::error::ec::EC;
use crate::m_error;
use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;

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
        .map_err(
            |e| m_error!(EC::EncodeErr, "error when encoding json", e)
        )
}

pub fn from_json_str<D: DeserializeOwned>(s: &str) -> RS<D> {
    serde_json::from_str(s)
        .map_err(|e|
            m_error!(EC::DecodeErr, "error when decoding json", e)
        )
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