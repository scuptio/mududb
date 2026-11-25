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

    mudu_client::management::install_app_package(server, mpk_binary)
        .await
        .map_err(|e| anyhow::anyhow!("install failed: {e}"))
        .with_context(|| format!("failed to install {} to {}", path.display(), server))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn upload_failure_preserves_package_and_server_context() -> anyhow::Result<()> {
        let path = mudu_sys::env_var::temp_dir().join(format!(
            "mpm-install-error-{}.mpk",
            mudu_utils::oid::gen_oid()
        ));
        mudu_sys::fs::sync::sync_write(&path, b"package bytes")?;
        let path_for_future = path.clone();
        let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            super::install_package("[invalid-address", &path_for_future).await
        })?;
        mudu_sys::fs::sync::sync_remove_file(&path)?;
        let error = match result {
            Err(error) => error,
            Ok(()) => anyhow::bail!("invalid HTTP address must fail"),
        };
        let chain = format!("{error:#}");
        assert!(chain.contains("failed to install"), "{chain}");
        assert!(chain.contains("[invalid-address"), "{chain}");
        assert!(chain.contains("install failed"), "{chain}");
        Ok(())
    }
}
