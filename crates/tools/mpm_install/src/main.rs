//! `mpm-install` — MuduDB package installer.
//!
//! A small npm-style command-line tool for installing `.mpk` application
//! packages into a running MuduDB server. Configuration can be provided via
//! an `mpm.cfg` file and overridden by command-line flags.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

mod install;
mod mpm_cfg;

use mpm_cfg::{MpmCfg, resolve_package, resolve_server};

/// Command-line arguments for `mpm-install`.
#[derive(Parser, Debug)]
#[command(name = "mpm-install")]
#[command(version)]
#[command(about = "MuduDB package installer")]
#[command(arg_required_else_help = true)]
struct Cli {
    /// Path to an `mpm.cfg` configuration file.
    #[arg(long = "cfg", global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    /// MuduDB HTTP management address, e.g. `127.0.0.1:8300`.
    #[arg(short, long, global = true, value_name = "ADDR")]
    server: Option<String>,

    /// Path to the `.mpk` package to install.
    #[arg(value_name = "PACKAGE")]
    package: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    mudu_sys::task::async_::block_on_async_current(async {
        if let Err(err) = run(cli).await {
            eprintln!("{err:?}");
            mudu_sys::process::exit(1);
        }
    });
}

async fn run(cli: Cli) -> Result<()> {
    let file_config =
        MpmCfg::load(cli.config.as_deref()).with_context(|| "failed to load configuration")?;
    let server = resolve_server(cli.server.as_deref(), file_config.server.as_deref());

    let package = resolve_package(cli.package.as_deref(), file_config.package.as_deref())
        .with_context(|| "failed to resolve package path")?;

    install::install_package(&server, &package)
        .await
        .with_context(|| "installation failed")?;

    println!("installed {} to {} successfully", package.display(), server);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_accepts_config_long_flag() {
        let cli = Cli::try_parse_from(["mpm-install", "--cfg", "./mpm.cfg", "wallet.mpk"]).unwrap();
        assert_eq!(
            cli.config.as_deref(),
            Some(std::path::Path::new("./mpm.cfg"))
        );
        assert_eq!(cli.package.as_deref(), Some("wallet.mpk"));
    }

    #[test]
    fn cli_accepts_global_server() {
        let cli =
            Cli::try_parse_from(["mpm-install", "--server", "192.168.1.1:8300", "wallet.mpk"])
                .unwrap();
        assert_eq!(cli.server.as_deref(), Some("192.168.1.1:8300"));
        assert_eq!(cli.package.as_deref(), Some("wallet.mpk"));
    }

    #[test]
    fn cli_requires_package() {
        assert!(Cli::try_parse_from(["mpm-install"]).is_err());
    }
}

#[cfg(test)]
mod run_tests {
    use super::*;
    use std::io::{Read, Write};

    /// A one-shot mock HTTP server answering `{"ok":true,"data":{}}`.
    fn start_mock_http_server() -> String {
        let listener = mudu_sys::net::sync::bind_tcp("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        mudu_sys::task::sync::spawn_thread(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let _ = socket.read(&mut buf);
            let body = r#"{"ok":true,"data":{}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes());
        })
        .unwrap();
        addr
    }

    #[test]
    fn run_installs_package_over_http() {
        let addr = start_mock_http_server();
        let dir = mudu_sys::env_var::temp_dir().join(format!("mpm_install_{}", std::process::id()));
        mudu_sys::fs::sync::sync_create_dir_all(&dir).unwrap();
        let mpk = dir.join("demo.mpk");
        mudu_sys::fs::sync::sync_write(&mpk, b"mpk-bytes").unwrap();
        let cli = Cli {
            config: None,
            server: Some(addr),
            package: Some(mpk.to_string_lossy().to_string()),
        };
        mudu_sys::task::async_::block_on_tokio_current_thread(async move { run(cli).await })
            .unwrap()
            .unwrap();
        let _ = mudu_sys::fs::sync::sync_remove_dir_all(&dir);
    }

    #[test]
    fn run_fails_for_missing_package_file() {
        let cli = Cli {
            config: None,
            server: Some("127.0.0.1:1".to_string()),
            package: Some("/nonexistent/never-there.mpk".to_string()),
        };
        mudu_sys::task::async_::block_on_tokio_current_thread(async move { run(cli).await })
            .unwrap()
            .unwrap_err();
    }

    #[test]
    fn run_fails_for_missing_config_file() {
        let cli = Cli {
            config: Some(PathBuf::from("/nonexistent/mpm.cfg")),
            server: None,
            package: Some("x.mpk".to_string()),
        };
        mudu_sys::task::async_::block_on_tokio_current_thread(async move { run(cli).await })
            .unwrap()
            .unwrap_err();
    }
}
