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
