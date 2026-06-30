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
        std::env::current_dir().map_err(|e| {
            mudu::mudu_error!(mudu::error::ErrorCode::from(&e), "get current dir error", e)
        })
    }

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
    fn args_os_is_non_empty() {
        let args = EnvVar::args_os();
        assert!(!args.is_empty());
    }
}
