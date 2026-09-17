//! The `mudud` server library.
//!
//! This crate exposes the configuration loading, argument parsing, and serve
//! logic used by the `mudud` binary so it can be unit-tested without spawning
//! a full server process.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

use clap::Parser;
use mudu::common::result::RS;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, init_mudud_cfg, load_mudud_cfg};
use mudu_sys::task::async_::wait_for_shutdown_signal;
use mudu_sys::task::sync::{SJoinHandle, spawn_thread_named};
use mudu_utils::notifier::{Notifier, Waiter, notify_wait};
use tracing::info;

/// Command-line arguments for `mudud`.
#[derive(Debug, Parser)]
#[command(name = "mudud", version, about = "MuduDB server")]
pub struct Args {
    #[command(subcommand)]
    /// Subcommand to run. When `None`, the server is started by default.
    pub command: Option<Command>,
}

/// Available `mudud` subcommands.
#[derive(Debug, Parser)]
pub enum Command {
    /// Run the MuduDB server (default behavior).
    Serve(ServeArgs),
    /// Write a default configuration file to the current directory.
    InitCfg,
}

/// Arguments for the `serve` subcommand.
#[derive(Debug, Parser, Default)]
pub struct ServeArgs {
    /// Path to mududb configuration TOML file.
    #[arg(short = 'c', long = "cfg", value_name = "FILE")]
    pub cfg_path: Option<String>,
}

/// Load configuration and run the backend until shutdown.
pub fn serve(args: ServeArgs) -> RS<()> {
    let (stop_notifier, stop_waiter) = notify_wait();
    let signal_thread = spawn_signal_listener(stop_notifier.clone())?;
    let serve_result = serve_with_stop(args, stop_waiter);
    stop_notifier.notify_all();
    let _ = signal_thread.join();
    serve_result
}

/// Load configuration and run the backend with an externally supplied stop
/// waiter.
///
/// This is useful for tests that want to drive shutdown without installing a
/// signal listener.
pub fn serve_with_stop(args: ServeArgs, stop_waiter: Waiter) -> RS<()> {
    serve_with_stop_and_runner(args, stop_waiter, Backend::sync_serve_with_stop)
}

/// Load configuration and run the backend using a mocked runner.
///
/// `runner` receives the loaded configuration and stop waiter and returns when
/// the server shuts down. Tests can supply a closure to verify the serve/stop
/// path without starting real network listeners.
pub fn serve_with_stop_and_runner<F>(args: ServeArgs, stop_waiter: Waiter, runner: F) -> RS<()>
where
    F: FnOnce(MuduDBCfg, Waiter) -> RS<()>,
{
    let cfg = load_mudud_cfg(args.cfg_path)?;
    info!(
        server_mode = ?cfg.server_mode,
        component_target = ?cfg.component_target(),
        listen_ip = %cfg.listen_ip,
        http_listen_port = cfg.http_listen_port,
        pg_listen_port = cfg.pg_listen_port,
        tcp_listen_port = cfg.tcp_listen_port,
        http_worker_threads = cfg.http_worker_threads,
        enable_async = cfg.enable_async,
        routing_mode = ?cfg.routing_mode,
        data_path = %cfg.db_path,
        mpk_path = %cfg.mpk_path,
        "mudud starting"
    );
    runner(cfg, stop_waiter)
}

/// Write a default configuration file to the current directory.
pub fn init_config() -> RS<()> {
    init_mudud_cfg()
}

/// Spawn a background thread that waits for a shutdown signal.
pub fn spawn_signal_listener(stop: Notifier) -> RS<SJoinHandle<()>> {
    spawn_thread_named("mudud-signal-listener", move || {
        wait_for_shutdown_signal(stop)
    })
    .map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Thread,
            "spawn signal listener error",
            e
        )
    })
}

#[cfg(test)]
mod cli_test;

#[cfg(test)]
mod serve_test;
