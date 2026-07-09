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
    log_setup_ex("info", "", false);
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
        }
    }
}
