mod desc;
mod parser;
mod procedure;
mod render;

#[cfg(test)]
mod tests;

use crate::assemblyscript::desc::gen_procedure_desc_list;
use crate::assemblyscript::parser::discover_procedures;
use crate::assemblyscript::render::{render_adapter_source, render_rust_wrapper, render_wit};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::json::to_json_str;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Transpile AssemblyScript source code to Mudu procedure adapter artifacts.
///
/// The primary output is an AssemblyScript adapter module with generated
/// `adapter_P` exports. Sibling `.rs` and `.wit` files contain the Rust P2
/// wrapper and procedure-specific WIT interfaces required by component
/// composition.
pub fn transpile_assemblyscript<I: AsRef<Path>, O: AsRef<Path>>(
    input: I,
    output: O,
    module_name: String,
    verbose: bool,
    opt_output_desc_file: Option<String>,
) -> i32 {
    let r = _transpile_assemblyscript(input, output, module_name, verbose, opt_output_desc_file);
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("transpile error: {}", e);
            e.ec().to_u32() as i32
        }
    }
}

fn _transpile_assemblyscript<I: AsRef<Path>, O: AsRef<Path>>(
    input: I,
    output: O,
    module_name: String,
    verbose: bool,
    opt_output_desc_file: Option<String>,
) -> RS<()> {
    let code = mudu_sys::fs::sync::sync_read_to_string(input.as_ref())
        .map_err(|e| m_error!(EC::IOErr, "read assemblyscript source code error", e))?;
    let procedures = discover_procedures(&code)?;
    if procedures.is_empty() {
        return Err(m_error!(
            EC::ParseErr,
            "no AssemblyScript procedure marked with /**mudu-proc*/ found"
        ));
    }

    let adapter_source = render_adapter_source(input.as_ref(), output.as_ref(), &procedures)?;
    mudu_sys::fs::sync::sync_write(output.as_ref(), adapter_source.as_bytes())
        .map_err(|e| m_error!(EC::IOErr, "write assemblyscript adapter source error", e))?;

    let output_path = output.as_ref();
    let rust_path = sibling_with_extension(output_path, "rs");
    let wit_path = sibling_with_extension(output_path, "wit");

    let rust_source = render_rust_wrapper(&procedures, &module_name)?;
    mudu_sys::fs::sync::sync_write(&rust_path, rust_source.as_bytes())
        .map_err(|e| m_error!(EC::IOErr, "write assemblyscript rust wrapper error", e))?;

    mudu_sys::fs::sync::sync_write(&wit_path, render_wit(&procedures).as_bytes())
        .map_err(|e| m_error!(EC::IOErr, "write assemblyscript procedure wit error", e))?;

    let desc_path = if let Some(desc_file) = opt_output_desc_file {
        let proc_desc_list = gen_procedure_desc_list(&module_name, &procedures);
        let modules = HashMap::from_iter(vec![(module_name.clone(), proc_desc_list)]);
        let package_desc = ModProcDesc::new(modules);
        let json = to_json_str(&package_desc)?;
        mudu_sys::fs::sync::sync_write(&desc_file, json)
            .map_err(|e| m_error!(EC::IOErr, "write assemblyscript procedure desc error", e))?;
        Some(desc_file)
    } else {
        None
    };

    if verbose {
        if let Some(desc_path) = desc_path {
            println!(
                "Successfully transpiled AssemblyScript, \nwrote {},\n{}, \n{},\n{}\n",
                output_path.display(),
                rust_path.display(),
                wit_path.display(),
                desc_path
            );
        } else {
            println!(
                "Successfully transpiled AssemblyScript, \nwrote {},\n{}, \n{}\n",
                output_path.display(),
                rust_path.display(),
                wit_path.display()
            );
        }
    }
    Ok(())
}

fn sibling_with_extension(path: &Path, extension: &str) -> PathBuf {
    let mut path = path.to_path_buf();
    path.set_extension(extension);
    path
}
