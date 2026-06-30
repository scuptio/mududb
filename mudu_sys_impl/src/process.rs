#![allow(missing_docs)]
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Child, ExitStatus, Output, Stdio};

pub fn exit(code: i32) -> ! {
    crate::default_env().process().exit(code)
}

/// Wrapper around [`std::process::Command`] so that callers outside of
/// `mudu_sys` can spawn subprocesses without triggering the disallowed-method
/// lint. Use this instead of `std::process::Command`.
pub struct Command {
    inner: std::process::Command,
}

impl Command {
    #[allow(clippy::disallowed_methods)]
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            inner: std::process::Command::new(program),
        }
    }

    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.inner.arg(arg);
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.inner.args(args);
        self
    }

    pub fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.inner.env(key, val);
        self
    }

    pub fn env_remove<K: AsRef<OsStr>>(&mut self, key: K) -> &mut Self {
        self.inner.env_remove(key);
        self
    }

    pub fn env_clear(&mut self) -> &mut Self {
        self.inner.env_clear();
        self
    }

    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.inner.current_dir(dir);
        self
    }

    pub fn stdin(&mut self, cfg: Stdio) -> &mut Self {
        self.inner.stdin(cfg);
        self
    }

    pub fn stdout(&mut self, cfg: Stdio) -> &mut Self {
        self.inner.stdout(cfg);
        self
    }

    pub fn stderr(&mut self, cfg: Stdio) -> &mut Self {
        self.inner.stderr(cfg);
        self
    }

    pub fn output(&mut self) -> std::io::Result<Output> {
        self.inner.output()
    }

    pub fn status(&mut self) -> std::io::Result<ExitStatus> {
        self.inner.status()
    }

    pub fn spawn(&mut self) -> std::io::Result<Child> {
        self.inner.spawn()
    }
}

#[cfg(all(test, not(miri)))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::Command;

    #[test]
    fn command_echo_output() {
        let output = Command::new("echo").arg("hello").output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "hello");
    }

    #[test]
    fn command_args() {
        let output = Command::new("printf")
            .args(["%s", "a", "b"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "ab");
    }

    #[test]
    fn command_env() {
        let output = Command::new("env")
            .env("MUDU_TEST_VAR", "value")
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("MUDU_TEST_VAR=value"));
    }

    #[test]
    fn command_current_dir() {
        let tmp = std::env::temp_dir();
        let output = Command::new("pwd").current_dir(&tmp).output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap().trim().to_string();
        assert_eq!(stdout, tmp.canonicalize().unwrap_or(tmp).to_string_lossy());
    }

    #[test]
    fn command_status_false() {
        let status = Command::new("false").status().unwrap();
        assert!(!status.success());
    }

    #[test]
    fn command_missing_program() {
        let err = Command::new("definitely_not_a_program_12345")
            .output()
            .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn command_spawn_wait() {
        let mut child = Command::new("true").spawn().unwrap();
        let status = child.wait().unwrap();
        assert!(status.success());
    }

    #[test]
    fn command_env_remove() {
        let output = Command::new("env")
            .env("MUDU_TEST_VAR", "value")
            .env_remove("MUDU_TEST_VAR")
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!stdout.contains("MUDU_TEST_VAR"));
    }

    #[test]
    fn command_env_clear() {
        let output = Command::new("env")
            .env("MUDU_TEST_VAR", "value")
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!stdout.contains("MUDU_TEST_VAR"));
    }
}
