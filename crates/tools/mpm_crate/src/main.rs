//! Command-line binary for the `mpm_crate` crate.
//!
//! `mpm-crate` scaffolds new MuduDB `.mpk` application projects for the
//! supported guest languages (Rust, AssemblyScript, C# and Python), the way
//! `npm create` / `cargo new` / `dotnet new` do.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use mpm_crate::scaffold::{Lang, ScaffoldArgs, ScaffoldReport, scaffold};

/// Command-line arguments for `mpm-crate`.
#[derive(Parser, Debug)]
#[command(name = "mpm-crate")]
#[command(version)]
#[command(about = "Create a new MuduDB .mpk application project from a template")]
#[command(arg_required_else_help = true)]
struct Cli {
    /// Project name (also the directory name): a lowercase letter followed by
    /// lowercase letters, digits or '-'.
    #[arg(value_name = "NAME")]
    name: String,

    /// Template language: rust (rs), assemblyscript (as), csharp (cs),
    /// python (py), c (cc/cpp) or go (golang).
    #[arg(long = "lang", value_name = "LANG")]
    lang: Lang,

    /// Parent directory in which to create the project directory (default:
    /// the current directory).
    #[arg(long = "path", value_name = "DIR")]
    path: Option<PathBuf>,

    /// Path to a mududb repository checkout; SDK dependencies become path
    /// dependencies instead of published-package placeholders (not used for
    /// csharp projects).
    #[arg(long = "sdk-path", value_name = "REPO")]
    sdk_path: Option<PathBuf>,
}

fn print_report(report: &ScaffoldReport) {
    let names = &report.names;
    println!(
        "Created {} project '{}' in '{}'",
        report.lang,
        names.project_name,
        report.project_dir.display()
    );
    println!();
    let mut files = report.files.clone();
    files.sort();
    for file in &files {
        println!("  {}", file.display());
    }
    let mpk = match report.lang {
        Lang::Python => format!("target/py-guest/{}.mpk", names.module_name),
        Lang::Go => format!("target/wasip2/{}.mpk", names.module_name),
        _ => format!("target/wasm32-wasip2/release/{}.mpk", names.module_name),
    };
    println!();
    println!("Next steps:");
    println!("  cd {}", report.project_dir.display());
    println!("  cargo make package");
    println!("  mpm-install {mpk}");
    println!(
        "  mcli --http-addr 127.0.0.1:8300 app-invoke --app {0} --module {0} \\",
        names.module_name
    );
    println!("    --proc create_item --json '{{\"item_id\": 2, \"name\": \"pear\"}}'");
    println!();
    println!(
        "See {}/readme.md for toolchain requirements and how to extend the project.",
        report.project_dir.display()
    );
}

fn run(cli: Cli) -> Result<()> {
    let args = ScaffoldArgs {
        name: cli.name,
        lang: cli.lang,
        path: cli.path,
        sdk_path: cli.sdk_path,
    };
    let report = scaffold(&args)?;
    print_report(&report);
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("Error: {err:?}");
        mudu_sys::process::exit(1);
    }
}

#[cfg(test)]
mod cli_test;
