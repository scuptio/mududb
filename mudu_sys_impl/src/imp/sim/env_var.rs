use mudu::common::result::RS;
use std::ffi::OsString;
use std::path::PathBuf;

pub struct EnvVar;

impl EnvVar {
    pub fn var(_key: &str) -> Option<String> {
        panic!("[sim] EnvVar::var not implemented")
    }

    pub fn var_os(_key: &str) -> Option<OsString> {
        panic!("[sim] EnvVar::var_os not implemented")
    }

    pub fn set_var(_key: &str, _value: &str) {
        panic!("[sim] EnvVar::set_var not implemented")
    }

    pub fn remove_var(_key: &str) {
        panic!("[sim] EnvVar::remove_var not implemented")
    }

    pub fn temp_dir() -> PathBuf {
        panic!("[sim] EnvVar::temp_dir not implemented")
    }

    pub fn current_dir() -> RS<PathBuf> {
        Err(mudu::m_error!(
            mudu::error::ec::EC::NotImplemented,
            "[sim] EnvVar::current_dir"
        ))
    }

    pub fn args_os() -> Vec<OsString> {
        panic!("[sim] EnvVar::args_os not implemented")
    }
}
