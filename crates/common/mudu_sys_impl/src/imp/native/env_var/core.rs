use mudu::common::result::RS;
use std::ffi::OsString;
use std::path::PathBuf;

/// Stateless environment variable operations over `std::env`.
pub struct EnvVar;

impl EnvVar {
    /// Returns the value of environment variable `key`, or `None` if it is
    /// not set or not valid Unicode.
    pub fn var(key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    /// Returns the raw [`OsString`] value of environment variable `key`, or
    /// `None` if it is not set.
    pub fn var_os(key: &str) -> Option<OsString> {
        std::env::var_os(key)
    }

    /// Sets environment variable `key` to `value` for the current process.
    pub fn set_var(key: &str, value: &str) {
        std::env::set_var(key, value);
    }

    /// Removes environment variable `key` from the current process.
    pub fn remove_var(key: &str) {
        std::env::remove_var(key);
    }

    /// Returns the path of the system temporary directory.
    pub fn temp_dir() -> PathBuf {
        std::env::temp_dir()
    }

    /// Returns the current working directory of the process.
    pub fn current_dir() -> RS<PathBuf> {
        std::env::current_dir().map_err(|e| {
            mudu::mudu_error!(mudu::error::ErrorCode::from(&e), "get current dir error", e)
        })
    }

    /// Returns the home directory of the current user (`$HOME`), if set.
    pub fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(PathBuf::from)
    }

    /// Returns the command-line arguments the process was started with.
    pub fn args_os() -> Vec<OsString> {
        std::env::args_os().collect()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]

    use super::*;
    use std::cell::Cell;

    thread_local! {
        static COUNTER: Cell<u64> = const { Cell::new(0) };
    }

    fn unique_key(prefix: &str) -> String {
        let n = COUNTER.with(|c| {
            let n = c.get();
            c.set(n + 1);
            n
        });
        format!("{}_{}_{}", prefix, std::process::id(), n)
    }

    #[test]
    fn var_and_var_os_return_none_for_missing_key() {
        let key = unique_key("missing");
        assert!(EnvVar::var(&key).is_none());
        assert!(EnvVar::var_os(&key).is_none());
    }

    #[test]
    fn set_and_get_var_roundtrip() {
        let key = unique_key("roundtrip");
        EnvVar::set_var(&key, "value");
        assert_eq!(EnvVar::var(&key).as_deref(), Some("value"));
        EnvVar::remove_var(&key);
    }

    #[test]
    fn var_os_roundtrips_os_string() {
        let key = unique_key("os_roundtrip");
        let value = OsString::from("os_value");
        std::env::set_var(&key, &value);
        assert_eq!(EnvVar::var_os(&key), Some(value));
        EnvVar::remove_var(&key);
    }

    #[test]
    fn remove_var_unsets_value() {
        let key = unique_key("remove");
        EnvVar::set_var(&key, "value");
        assert!(EnvVar::var(&key).is_some());
        EnvVar::remove_var(&key);
        assert!(EnvVar::var(&key).is_none());
    }

    #[test]
    fn temp_dir_returns_absolute_path() {
        let temp = EnvVar::temp_dir();
        assert!(temp.is_absolute());
    }

    #[test]
    fn current_dir_returns_ok_absolute_path() {
        let current = EnvVar::current_dir();
        assert!(current.is_ok());
        assert!(current.unwrap().is_absolute());
    }

    #[test]
    fn home_dir_returns_absolute_path_when_home_set() {
        let home = EnvVar::home_dir();
        if std::env::var("HOME").is_ok() {
            assert!(home.is_some());
            assert!(home.unwrap().is_absolute());
        } else {
            assert!(home.is_none());
        }
    }

    #[test]
    fn args_os_is_non_empty() {
        let args = EnvVar::args_os();
        assert!(!args.is_empty());
    }
}
