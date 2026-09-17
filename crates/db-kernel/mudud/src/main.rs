//! The `mudud` server binary.
//!
//! This is the main entry point for running a MuduDB server process. It loads
//! the configuration, sets up logging, and drives the runtime backend until a
//! shutdown signal is received.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

use clap::Parser;
use mudu_utils::log::log_setup_ex;
use mudud::{Args, Command, ServeArgs, init_config, serve};
use tracing::error;

fn main() {
    // Honor RUST_LOG (e.g. `RUST_LOG=mudu_kernel=debug mudud serve`) as a
    // tracing filter on top of the default info level. A filter that starts
    // with target-specific directives gets an explicit `info` base level,
    // otherwise unlisted targets would be silenced entirely.
    let rust_log = mudu_sys::env_var::var("RUST_LOG").unwrap_or_default();
    let first_segment = rust_log.split(',').next().unwrap_or("");
    let filter = if !rust_log.is_empty() && first_segment.contains('=') {
        format!("info,{rust_log}")
    } else {
        rust_log
    };
    log_setup_ex("info", &filter, false);
    let args = Args::parse();
    let r = match args.command {
        Some(Command::InitCfg) => init_config(),
        Some(Command::Serve(serve_args)) => serve(serve_args),
        None => serve(ServeArgs::default()),
    };
    match r {
        Ok(_) => {}
        Err(e) => {
            error!("mududb run error: {}", e);
            // Exit non-zero so supervisors and benchmark harnesses can tell a
            // startup failure (e.g. listen port already in use) apart from a
            // clean shutdown.
            mudu_sys::process::exit(1);
        }
    }
}
