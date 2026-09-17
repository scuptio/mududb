//! Command-line interface for the `mtp` transpiler binary.

use crate::common::type_registry::TypeRegistry;
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

    /// WIT file (or directory of `.wit` files) declaring user-defined
    /// procedure types; repeatable. Used by the byte-pipe front-ends
    /// (AssemblyScript, Python, C#, C, Go).
    #[arg(long = "type-wit", short = 'T')]
    pub type_wit: Vec<String>,

    /// Import location of the project's mgen-generated message types,
    /// interpreted per front-end (Go: module import path, e.g.
    /// "my_module/gentypes"; Python: module path; C#: namespace; C:
    /// include path; AssemblyScript: import specifier); required when a
    /// procedure signature references a user-defined type from --type-wit.
    #[arg(long = "type-import")]
    pub type_import: Option<String>,

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

    /// Transpile Python source code
    #[command(alias = "py")]
    Python,

    /// Transpile C# source code
    #[command(alias = "cs")]
    Csharp,

    /// Transpile C source code
    C,

    /// Transpile Go source code
    #[command(alias = "golang")]
    Go,
}

/// Execute the CLI command based on parsed arguments.
pub fn execute(args: Args) -> Result<(), String> {
    if args.verbose {
        println!("Mudu Transpiler started");
    }

    let type_registry = if args.type_wit.is_empty() {
        None
    } else {
        match TypeRegistry::from_wit_paths(&args.type_wit) {
            Ok(registry) => {
                if args.verbose {
                    println!(
                        "Loaded {} user-defined type(s) from --type-wit",
                        registry.len()
                    );
                }
                Some(registry)
            }
            Err(e) => return Err(format!("failed to load --type-wit: {}", e)),
        }
    };

    match &args.command {
        CommandType::Rust => handle_rust(args.clone()),
        CommandType::AssemblyScript => handle_assemblyscript(args.clone(), type_registry.as_ref()),
        CommandType::Python => handle_python(args.clone(), type_registry.as_ref()),
        CommandType::Csharp => handle_csharp(args.clone(), type_registry.as_ref()),
        CommandType::C => handle_c(args.clone(), type_registry.as_ref()),
        CommandType::Go => handle_go(args.clone(), type_registry.as_ref()),
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
fn handle_assemblyscript(args: Args, type_registry: Option<&TypeRegistry>) -> Result<(), String> {
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
        type_registry,
        args.type_import.clone(),
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

/// Handle Python transpilation
fn handle_python(args: Args, type_registry: Option<&TypeRegistry>) -> Result<(), String> {
    if args.verbose {
        println!("Source language: Python");
        println!("Input file: {}", args.input);
        println!("Output file: {}", args.output);
    }
    let input_file = PathBuf::from(&args.input);
    let output_file = PathBuf::from(&args.output);
    let module = args.module.unwrap_or_else(|| "module".to_string());

    let ret = crate::python::transpile_python(
        &input_file,
        &output_file,
        module,
        args.verbose,
        args.package_desc,
        type_registry,
        args.type_import.clone(),
    );

    if ret == 0 {
        Ok(())
    } else {
        Err(format!(
            "Python transpilation failed with exit code: {}",
            ret
        ))
    }
}

/// Handle C# transpilation
fn handle_csharp(args: Args, type_registry: Option<&TypeRegistry>) -> Result<(), String> {
    if args.verbose {
        println!("Source language: C#");
        println!("Input file: {}", args.input);
        println!("Output file: {}", args.output);
    }
    let input_file = PathBuf::from(&args.input);
    let output_file = PathBuf::from(&args.output);
    let module = args.module.unwrap_or_else(|| "module".to_string());

    let ret = crate::csharp::transpile_csharp(
        &input_file,
        &output_file,
        module,
        args.verbose,
        args.package_desc,
        type_registry,
        args.type_import.clone(),
    );

    if ret == 0 {
        Ok(())
    } else {
        Err(format!("C# transpilation failed with exit code: {}", ret))
    }
}

/// Handle C transpilation
fn handle_c(args: Args, type_registry: Option<&TypeRegistry>) -> Result<(), String> {
    if args.verbose {
        println!("Source language: C");
        println!("Input file: {}", args.input);
        println!("Output file: {}", args.output);
    }
    let input_file = PathBuf::from(&args.input);
    let output_file = PathBuf::from(&args.output);
    let module = args.module.unwrap_or_else(|| "module".to_string());

    let ret = crate::c::transpile_c(
        &input_file,
        &output_file,
        module,
        args.verbose,
        args.package_desc,
        type_registry,
        args.type_import.clone(),
    );

    if ret == 0 {
        Ok(())
    } else {
        Err(format!("C transpilation failed with exit code: {}", ret))
    }
}

/// Handle Go transpilation
fn handle_go(args: Args, type_registry: Option<&TypeRegistry>) -> Result<(), String> {
    if args.verbose {
        println!("Source language: Go");
        println!("Input file: {}", args.input);
        println!("Output file: {}", args.output);
    }
    let input_file = PathBuf::from(&args.input);
    let output_file = PathBuf::from(&args.output);
    let module = args.module.unwrap_or_else(|| "module".to_string());

    let ret = crate::go::transpile_go(
        &input_file,
        &output_file,
        module,
        args.verbose,
        args.package_desc,
        type_registry,
        args.type_import.clone(),
    );

    if ret == 0 {
        Ok(())
    } else {
        Err(format!("Go transpilation failed with exit code: {}", ret))
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
