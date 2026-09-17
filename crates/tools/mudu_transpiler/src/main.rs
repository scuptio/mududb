//! Binary entry point for the `mtp` command-line tool.

use mudu_transpiler::mtp::main_inner;
use std::error::Error;

/// Mudu Transpiler (`mtp`) - transpile source code to Mudu procedures.
///
/// Supports: AssemblyScript, Python, Rust, C#, C, and Go.
fn main() -> Result<(), Box<dyn Error>> {
    main_inner(mudu_sys::env_var::args_os())?;
    Ok(())
}
