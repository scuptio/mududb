//! Environment variable and process environment helpers.
use mudu::common::result::RS;
use std::ffi::OsString;
use std::path::PathBuf;

/// Returns the value of environment variable `key`, or `None` if it is not
/// set or not valid Unicode.
pub fn var(key: &str) -> Option<String> {
    crate::default_env().env_var().var(key)
}

/// Returns the raw [`OsString`] value of environment variable `key`, or
/// `None` if it is not set.
pub fn var_os(key: &str) -> Option<OsString> {
    crate::default_env().env_var().var_os(key)
}

/// Sets environment variable `key` to `value` for the current process.
pub fn set_var(key: &str, value: &str) {
    crate::default_env().env_var().set_var(key, value);
}

/// Removes environment variable `key` from the current process.
pub fn remove_var(key: &str) {
    crate::default_env().env_var().remove_var(key);
}

/// Returns the path of the system temporary directory.
pub fn temp_dir() -> PathBuf {
    crate::default_env().env_var().temp_dir()
}

/// Returns the current working directory of the process.
pub fn current_dir() -> RS<PathBuf> {
    crate::default_env().env_var().current_dir()
}

/// Returns the home directory of the current user (`$HOME`), if set.
pub fn home_dir() -> Option<PathBuf> {
    crate::default_env().env_var().home_dir()
}

/// Returns the command-line arguments the process was started with.
pub fn args_os() -> Vec<OsString> {
    crate::default_env().env_var().args_os()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    #[test]
    fn env_var_facade_roundtrip_and_process_dirs() {
        let key = "MUDU_SYS_IMPL_ENV_VAR_FACADE_TEST";
        super::set_var(key, "value1");
        assert_eq!(super::var(key).as_deref(), Some("value1"));
        assert_eq!(
            super::var_os(key).unwrap(),
            std::ffi::OsString::from("value1")
        );
        super::remove_var(key);
        assert_eq!(super::var(key), None);

        let _temp = super::temp_dir();
        assert!(super::current_dir().unwrap().is_absolute());
        let _home = super::home_dir();
        assert!(!super::args_os().is_empty());
    }
}
