mod rust;
mod mtp;
mod test_mtp;

use std::error::Error;
use crate::mtp::main_inner;

/// Mudu Transpiler (mtp) - A tool to transpile source code to Mudu procedure
/// Supports: AssemblyScript, C#, Golang, Python, Rust
fn main() -> Result<(), Box<dyn Error>> {
    main_inner(std::env::args_os())
        .map_err(|e| { Box::new(e)} )?;
    Ok(())
}



