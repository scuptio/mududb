//! Installation logic for `mpm_install`.
//!
//! Reads a local `.mpk` archive and uploads it to a MuduDB server through the
//! HTTP management endpoint.

use std::path::Path;

use anyhow::{Context, Result};

/// Read the `.mpk` package from `path` and install it on the MuduDB server at
/// `server`.
///
/// `server` should be a plain `host:port` address such as `127.0.0.1:8300`;
/// the management API helper prepends `http://` internally.
pub async fn install_package(server: &str, path: &Path) -> Result<()> {
    let mpk_binary = mudu_sys::fs::sync::sync_read_all(path)
        .with_context(|| format!("failed to read package {}", path.display()))?;

    mudu_cli::management::install_app_package(server, mpk_binary)
        .await
        .map_err(|e| anyhow::anyhow!("install failed: {e}"))
        .with_context(|| format!("failed to install {} to {}", path.display(), server))?;

    Ok(())
}
