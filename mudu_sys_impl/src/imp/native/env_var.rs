use mudu::common::result::RS;
use std::ffi::OsString;
use std::path::PathBuf;

pub struct EnvVar;

impl EnvVar {
    pub fn var(key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    pub fn var_os(key: &str) -> Option<OsString> {
        std::env::var_os(key)
    }

    pub fn set_var(key: &str, value: &str) {
        std::env::set_var(key, value);
    }

    pub fn remove_var(key: &str) {
        std::env::remove_var(key);
    }

    pub fn temp_dir() -> PathBuf {
        std::env::temp_dir()
    }

    pub fn current_dir() -> RS<PathBuf> {
        std::env::current_dir()
            .map_err(|e| mudu::m_error!(mudu::error::ec::EC::IOErr, "get current dir error", e))
    }

    pub fn args_os() -> Vec<OsString> {
        std::env::args_os().collect()
    }
}
