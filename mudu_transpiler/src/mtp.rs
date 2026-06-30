//! Command-line interface for the `mtp` transpiler binary.

use clap::{ArgAction, Parser};
use mudu::common::result::RS;
use mudu_sys::process;
use std::path::PathBuf;

/// Command-line arguments structure for the Mudu Transpiler.
#[derive(Parser, Clone)]
#[command(
    name = "mtp",
    version = "1.0",
    author = "scuptio",
    about = "Mudu Transpiler (mtp), transpile source code to Mudu procedure",
    long_about = "Transpiles source code from supported programming languages to Mudu procedure format"
)]
pub struct Args {
    /// Subcommand specifying the source language
    #[command(subcommand)]
    pub command: CommandType,

    /// Input file path
    #[arg(long = "input", short = 'i')]
    pub input: String,

    /// Output file path
    #[arg(long = "output", short = 'o')]
    pub output: String,

    /// MPK module name
    #[arg(short = 'm', long)]
    pub module: Option<String>,

    /// Source Rust code module name
    #[arg(long = "src-mod", short = 's')]
    pub src_mod: Option<String>,

    /// Destination Rust code module name
    #[arg(long = "dst-mod", short = 'd')]
    pub dst_mod: Option<String>,

    /// Enable compile to async (Rust-specific)
    #[arg(long = "async", short = 'a', action = ArgAction::SetTrue)]
    pub enable_async: bool,

    /// Custom type description file
    #[arg(long = "type-desc", short = 't')]
    pub type_desc_file: Option<String>,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Procedure description file
    #[arg(long = "package-desc", short = 'p')]
    pub package_desc: Option<String>,
}

/// Supported source language subcommands.
#[derive(Parser, Clone)]
pub enum CommandType {
    /// Transpile Rust source code
    #[command(alias = "rs")]
    Rust,

    /// Transpile AssemblyScript source code
    #[command(alias = "as")]
    AssemblyScript,
}

/// Execute the CLI command based on parsed arguments.
pub fn execute(args: Args) -> Result<(), String> {
    if args.verbose {
        println!("Mudu Transpiler started");
    }

    match &args.command {
        CommandType::Rust => handle_rust(args.clone()),
        CommandType::AssemblyScript => handle_assemblyscript(args.clone()),
    }
}

/// Handle Rust transpilation
fn handle_rust(args: Args) -> Result<(), String> {
    if args.verbose {
        println!("Source language: Rust");
        println!("Input file: {}", args.input);
        println!("Output file: {}", args.output);
    }

    let input_file = PathBuf::from(&args.input);
    let output_file = PathBuf::from(&args.output);
    let module = args.module.unwrap_or_else(|| "module".to_string());

    let ret = crate::rust::transpile_rust(crate::rust::TranspileRustOptions {
        input: &input_file,
        output: &output_file,
        module_name: module,
        verbose: args.verbose,
        enable_async: args.enable_async,
        src_mod: args.src_mod,
        dst_mod: args.dst_mod,
        output_desc_file: args.package_desc,
        custom_type_def_file: args.type_desc_file,
    });

    if ret == 0 {
        Ok(())
    } else {
        Err(format!("Rust transpilation failed with exit code: {}", ret))
    }
}

/// Handle AssemblyScript transpilation
fn handle_assemblyscript(args: Args) -> Result<(), String> {
    if args.verbose {
        println!("Source language: AssemblyScript");
        println!("Input file: {}", args.input);
        println!("Output file: {}", args.output);
    }
    let input_file = PathBuf::from(&args.input);
    let output_file = PathBuf::from(&args.output);
    let module = args.module.unwrap_or_else(|| "module".to_string());

    let ret = crate::assemblyscript::transpile_assemblyscript(
        &input_file,
        &output_file,
        module,
        args.verbose,
        args.package_desc,
    );

    if ret == 0 {
        Ok(())
    } else {
        Err(format!(
            "AssemblyScript transpilation failed with exit code: {}",
            ret
        ))
    }
}

/// Parse command-line arguments from `args` and run the transpiler.
///
/// Errors are returned as a string so tests can assert on them without the
/// binary terminating the process.
pub fn run<I, T>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args = Args::try_parse_from(args).map_err(|e| e.to_string())?;
    execute(args)
}

/// Parse command-line arguments from `args` and run the transpiler.
///
/// This is exposed as a library entry point so it can be exercised from
/// integration tests without spawning a subprocess. On error it prints to
/// stderr and exits the process with code 1.
pub fn main_inner<I, T>(args: I) -> RS<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    if let Err(e) = run(args) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
    Ok(())
}
