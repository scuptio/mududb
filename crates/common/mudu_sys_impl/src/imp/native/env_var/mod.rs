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
    /// Creates the environment variable subsystem handle.
    pub fn new() -> Self {
        Self
    }

    /// Returns the value of environment variable `key`, or `None` if it is
    /// not set or not valid Unicode.
    pub fn var(&self, key: &str) -> Option<String> {
        EnvVar::var(key)
    }

    /// Returns the raw [`OsString`] value of environment variable `key`, or
    /// `None` if it is not set.
    pub fn var_os(&self, key: &str) -> Option<OsString> {
        EnvVar::var_os(key)
    }

    /// Sets environment variable `key` to `value` for the current process.
    pub fn set_var(&self, key: &str, value: &str) {
        EnvVar::set_var(key, value);
    }

    /// Removes environment variable `key` from the current process.
    pub fn remove_var(&self, key: &str) {
        EnvVar::remove_var(key);
    }

    /// Returns the path of the system temporary directory.
    pub fn temp_dir(&self) -> PathBuf {
        EnvVar::temp_dir()
    }

    /// Returns the current working directory of the process.
    pub fn current_dir(&self) -> RS<PathBuf> {
        EnvVar::current_dir()
    }

    /// Returns the home directory of the current user (`$HOME`), if set.
    pub fn home_dir(&self) -> Option<PathBuf> {
        EnvVar::home_dir()
    }

    /// Returns the command-line arguments the process was started with.
    pub fn args_os(&self) -> Vec<OsString> {
        EnvVar::args_os()
    }
}
