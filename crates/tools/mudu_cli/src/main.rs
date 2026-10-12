#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! `mcli` — command-line client for MuduDB.
//!
//! Reads command-line arguments, sends TCP or HTTP requests to a MuduDB
//! server, and prints the response as JSON or an interactive table.

mod cli;
mod dispatch;
mod io_util;
mod repl_status;
mod shell;

pub(crate) use cli::*;
pub(crate) use dispatch::*;
pub(crate) use io_util::*;
#[cfg(test)]
pub(crate) use repl_status::*;
#[cfg(test)]
pub(crate) use shell::*;

use clap::Parser;
use mudu::common::result::RS;

fn main() {
    let cli = Cli::parse();
    mudu_sys::task::async_::block_on_async_current(async {
        if let Err(err) = run(cli).await {
            eprintln!("{err}");
            mudu_sys::process::exit(1);
        }
    });
}

async fn run(cli: Cli) -> RS<()> {
    let compact = cli.compact;
    let table = cli.table;
    let no_table = cli.no_table;
    let output = run_with_connectors(cli, &RealJsonConnector, &RealAsyncConnector).await?;
    print_output(&output, compact, table, no_table)?;
    Ok(())
}

#[cfg(test)]
mod main_test;
