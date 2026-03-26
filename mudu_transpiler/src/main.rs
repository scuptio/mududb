mod mtp;
mod rust;
mod test_mtp;

use crate::mtp::main_inner;
use std::error::Error;

/// Mudu Transpiler (mtp) - A tool to transpile source code to Mudu procedure
/// Supports: AssemblyScript, C#, Golang, Python, Rust
fn main() -> Result<(), Box<dyn Error>> {
    main_inner(std::env::args_os()).map_err(|e| Box::new(e))?;
    Ok(())
}
