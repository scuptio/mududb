#![allow(clippy::disallowed_methods)]

use crate::common::result::RS;
use crate::error::ec::EC;
use crate::m_error;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;

pub fn to_toml_str<S: Serialize>(object: &S) -> RS<String> {
    let toml_string = toml::to_string_pretty(object)
        .map_err(|e| m_error!(EC::EncodeErr, "serialize to toml error", e))?;
    Ok(toml_string)
}

pub fn write_toml<S: Serialize, P: AsRef<Path>>(object: &S, path: P) -> RS<()> {
    let toml_string = to_toml_str(object)?;
    fs::write(path.as_ref(), toml_string).map_err(|e| {
        m_error!(
            EC::IOErr,
            format!("write to file {:?} error", path.as_ref()),
            e
        )
    })?;
    Ok(())
}

pub fn read_toml<D: DeserializeOwned, P: AsRef<Path>>(path: P) -> RS<D> {
    let s = fs::read_to_string(path.as_ref()).map_err(|e| {
        m_error!(
            EC::IOErr,
            format!("read toml file {:?} error", path.as_ref()),
            e
        )
    })?;
    let ret: D = toml::from_str::<D>(&s)
        .map_err(|e| m_error!(EC::DecodeErr, "decode from toml string error", e))?;
    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::{read_toml, to_toml_str, write_toml};
    use serde::{Deserialize, Serialize};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct DemoToml {
        id: u32,
        name: String,
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("mudu_toml_{name}_{suffix}.toml"))
    }

    #[test]
    fn toml_string_and_file_roundtrip() {
        let value = DemoToml {
            id: 7,
            name: "alice".to_string(),
        };
        let toml = to_toml_str(&value).unwrap();
        assert!(toml.contains("id = 7"));

        let path = temp_path("roundtrip");
        write_toml(&value, &path).unwrap();
        let loaded: DemoToml = read_toml(&path).unwrap();
        assert_eq!(loaded, value);
    }

    #[test]
    fn read_toml_rejects_invalid_input() {
        let path = temp_path("invalid");
        std::fs::write(&path, "not = [valid").unwrap();
        let err = read_toml::<DemoToml, _>(&path).unwrap_err();
        assert!(err.to_string().contains("DecodeErr"));
    }
}
