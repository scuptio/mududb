//! Configuration handling for `mpm_install`.
//!
//! Supports loading a `.toml` configuration file from either an explicit path,
//! the current directory (`./mpm.cfg`), or the user's home directory
//! (`~/.mududb/mpm.cfg`). Command-line arguments take precedence over file
//! values.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Default MuduDB HTTP management address.
pub const DEFAULT_SERVER: &str = "127.0.0.1:8300";

/// Default name of a per-project configuration file.
pub const PROJECT_CONFIG_NAME: &str = "mpm.cfg";

/// Default path of the user-level configuration file relative to the home
/// directory.
pub const USER_CONFIG_PATH: &str = ".mududb/mpm.cfg";

/// Configuration values that can be stored in an `mpm.cfg` file.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MpmCfg {
    /// MuduDB HTTP management address, e.g. `127.0.0.1:8300`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,

    /// Default package to install when no package argument is given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

impl MpmCfg {
    /// Load a configuration file from one of the standard locations.
    ///
    /// If `explicit_path` is given it is used exclusively. Otherwise the
    /// function looks for `./mpm.cfg` and then `~/.mududb/mpm.cfg`.
    pub fn load(explicit_path: Option<&Path>) -> Result<Self> {
        if let Some(path) = explicit_path {
            return Self::from_file(path)
                .with_context(|| format!("failed to read config file {}", path.display()));
        }

        if let Some(path) = project_config_path()
            && mudu_sys::fs::sync::sync_path_exists(&path)
        {
            return Self::from_file(&path)
                .with_context(|| format!("failed to read project config {}", path.display()));
        }

        if let Some(path) = user_config_path()
            && mudu_sys::fs::sync::sync_path_exists(&path)
        {
            return Self::from_file(&path)
                .with_context(|| format!("failed to read user config {}", path.display()));
        }

        Ok(Self::default())
    }

    /// Read a configuration from a specific TOML file.
    pub fn from_file<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        mudu_utils::toml::read_toml::<Self, _>(&path)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .with_context(|| format!("failed to parse {}", path.as_ref().display()))
    }
}

/// Return the path to the per-project configuration file in the current
/// working directory.
pub fn project_config_path() -> Option<PathBuf> {
    let current = mudu_sys::env_var::current_dir().ok()?;
    Some(current.join(PROJECT_CONFIG_NAME))
}

/// Return the path to the user-level configuration file.
pub fn user_config_path() -> Option<PathBuf> {
    let home = home_dir()?;
    Some(home.join(USER_CONFIG_PATH))
}

/// Return the user's home directory.
pub fn home_dir() -> Option<PathBuf> {
    mudu_sys::env_var::var("HOME")
        .or_else(|| mudu_sys::env_var::var("USERPROFILE"))
        .map(PathBuf::from)
}

/// Resolve the final server address.
///
/// CLI argument takes precedence, then config file, then the default.
/// Accepts both `host:port` and `http://host:port`; the scheme is stripped
/// before returning.
pub fn resolve_server(cli: Option<&str>, config: Option<&str>) -> String {
    let raw = cli.or(config).unwrap_or(DEFAULT_SERVER);
    strip_url_scheme(raw).to_string()
}

fn strip_url_scheme(addr: &str) -> &str {
    if let Some(rest) = addr.strip_prefix("http://") {
        return rest;
    }
    if let Some(rest) = addr.strip_prefix("https://") {
        return rest;
    }
    addr
}

/// Resolve the package path to install.
///
/// CLI argument takes precedence, then the `package` field in the config.
/// If the path does not exist and has no `.mpk` extension, a `.mpk` suffix
/// is appended.
pub fn resolve_package(cli: Option<&str>, config: Option<&str>) -> Result<PathBuf> {
    let raw = cli.or(config).ok_or_else(|| {
        anyhow::anyhow!("no package specified (provide an argument or set 'package' in mpm.cfg)")
    })?;

    let mut path = PathBuf::from(raw);
    if !mudu_sys::fs::sync::sync_path_exists(&path) && path.extension().is_none() {
        path.set_extension("mpk");
    }

    if !mudu_sys::fs::sync::sync_path_exists(&path) {
        anyhow::bail!("package not found: {}", path.display());
    }

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_server_prefers_cli_then_config_then_default() {
        assert_eq!(
            resolve_server(Some("1.2.3.4:1"), Some("5.6.7.8:2")),
            "1.2.3.4:1"
        );
        assert_eq!(resolve_server(None, Some("5.6.7.8:2")), "5.6.7.8:2");
        assert_eq!(resolve_server(None, None), DEFAULT_SERVER);
    }

    #[test]
    fn resolve_server_strips_http_scheme() {
        assert_eq!(
            resolve_server(Some("http://127.0.0.1:8300"), None),
            "127.0.0.1:8300"
        );
        assert_eq!(
            resolve_server(Some("https://127.0.0.1:8300"), None),
            "127.0.0.1:8300"
        );
    }

    #[test]
    fn resolve_package_requires_value() {
        assert!(resolve_package(None, None).is_err());
    }

    #[test]
    fn resolve_package_cli_overrides_config() {
        let got = resolve_package(Some("cli_pkg"), Some("cfg_pkg")).unwrap_err();
        assert!(got.to_string().contains("cli_pkg"));
    }
}
