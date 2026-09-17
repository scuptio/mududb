use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::Path;

pub fn to_toml_str<S: Serialize>(object: &S) -> RS<String> {
    toml::to_string_pretty(object)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "serialize to toml error", e))
}

pub fn write_toml<S: Serialize, P: AsRef<Path>>(object: &S, path: P) -> RS<()> {
    let toml_string = to_toml_str(object)?;
    mudu_sys::fs::sync::write(path.as_ref(), toml_string)?;
    Ok(())
}

pub fn read_toml<D: DeserializeOwned, P: AsRef<Path>>(path: P) -> RS<D> {
    let s = mudu_sys::fs::sync::read_to_string(path.as_ref())?;
    let ret: D = toml::from_str::<D>(&s)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode from toml string error", e))?;
    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::{read_toml, to_toml_str, write_toml};
    use mudu::error::ErrorCode;
    use serde::{Deserialize, Serialize};
    use std::time::UNIX_EPOCH;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct DemoToml {
        id: u32,
        name: String,
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let suffix = mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        mudu_sys::env_var::temp_dir().join(format!("mudu_toml_{name}_{suffix}.toml"))
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
        mudu_sys::fs::sync::write(&path, "not = [valid").unwrap();
        let err = read_toml::<DemoToml, _>(&path).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }
}
