use crate::imp::env::Sys;
use mudu::common::result::RS;
use std::ffi::OsString;
use std::path::PathBuf;

pub fn var(key: &str) -> Option<String> {
    Sys::var(key)
}

pub fn var_os(key: &str) -> Option<OsString> {
    Sys::var_os(key)
}

pub fn set_var(key: &str, value: &str) {
    Sys::set_var(key, value);
}

pub fn remove_var(key: &str) {
    Sys::remove_var(key);
}

pub fn temp_dir() -> PathBuf {
    Sys::temp_dir()
}

pub fn current_dir() -> RS<PathBuf> {
    Sys::current_dir()
}

pub fn args_os() -> Vec<OsString> {
    Sys::args_os()
}
