#![allow(missing_docs)]
use mudu::common::result::RS;
use std::ffi::OsString;
use std::path::PathBuf;

mod core;

pub use core::EnvVar;

/// Environment variable subsystem - native implementation.
pub struct SysEnvVar;

impl Default for SysEnvVar {
    fn default() -> Self {
        Self::new()
    }
}

impl SysEnvVar {
    pub fn new() -> Self {
        Self
    }

    pub fn var(&self, key: &str) -> Option<String> {
        EnvVar::var(key)
    }

    pub fn var_os(&self, key: &str) -> Option<OsString> {
        EnvVar::var_os(key)
    }

    pub fn set_var(&self, key: &str, value: &str) {
        EnvVar::set_var(key, value);
    }

    pub fn remove_var(&self, key: &str) {
        EnvVar::remove_var(key);
    }

    pub fn temp_dir(&self) -> PathBuf {
        EnvVar::temp_dir()
    }

    pub fn current_dir(&self) -> RS<PathBuf> {
        EnvVar::current_dir()
    }

    pub fn home_dir(&self) -> Option<PathBuf> {
        EnvVar::home_dir()
    }

    pub fn args_os(&self) -> Vec<OsString> {
        EnvVar::args_os()
    }
}
