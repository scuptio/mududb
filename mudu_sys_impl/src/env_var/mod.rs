//! Environment variable and process environment helpers.
#![allow(missing_docs)]
use mudu::common::result::RS;
use std::ffi::OsString;
use std::path::PathBuf;

pub fn var(key: &str) -> Option<String> {
    crate::default_env().env_var().var(key)
}

pub fn var_os(key: &str) -> Option<OsString> {
    crate::default_env().env_var().var_os(key)
}

pub fn set_var(key: &str, value: &str) {
    crate::default_env().env_var().set_var(key, value);
}

pub fn remove_var(key: &str) {
    crate::default_env().env_var().remove_var(key);
}

pub fn temp_dir() -> PathBuf {
    crate::default_env().env_var().temp_dir()
}

pub fn current_dir() -> RS<PathBuf> {
    crate::default_env().env_var().current_dir()
}

pub fn args_os() -> Vec<OsString> {
    crate::default_env().env_var().args_os()
}
