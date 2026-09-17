//! AssemblyScript front-end for the Mudu transpiler.
//!
//! This module discovers `/**mudu-proc*/` functions in TypeScript-compatible
//! AssemblyScript source and emits the byte-pipe adapter module plus the
//! procedure world WIT: the finished AssemblyScript component imports
//! `mududb:api/system` directly and exports one root-level `mp2-<kebab>`
//! `func(param: list<u8>) -> list<u8>` per procedure (the same architecture
//! as the C# guest).

mod desc;
mod parser;
mod procedure;
mod render;

// tree-sitter grammars are implemented in C and call foreign functions that Miri
// does not support, so skip these parser tests under Miri.
#[cfg(all(test, not(miri)))]
mod tests;

use crate::assemblyscript::desc::gen_procedure_desc_list;
use crate::assemblyscript::parser::discover_procedures;
use crate::assemblyscript::render::{render_adapter_source, render_wit};
use crate::common::desc::write_package_desc;
use crate::common::type_registry::TypeRegistry;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::path::Path;

/// Transpile AssemblyScript source code to Mudu procedure adapter artifacts.
///
/// The primary output is an AssemblyScript adapter module with generated
/// `mp2_P` byte-pipe exports. The sibling `.wit` file carries the procedure
/// world (`import mududb:api/system` plus one root-level `mp2-<kebab>`
/// byte-pipe export per procedure).
///
/// `type_registry` carries the user-defined types loaded from `--type-wit`;
/// a procedure signature referencing one of those types is decoded through
/// the project's mgen-generated AssemblyScript types module, whose import
/// specifier (e.g. `"./gentypes"`) is `type_import` (`--type-import`).
pub fn transpile_assemblyscript<I: AsRef<Path>, O: AsRef<Path>>(
    input: I,
    output: O,
    module_name: String,
    verbose: bool,
    opt_output_desc_file: Option<String>,
    type_registry: Option<&TypeRegistry>,
    type_import: Option<String>,
) -> i32 {
    let r = _transpile_assemblyscript(
        input,
        output,
        module_name,
        verbose,
        opt_output_desc_file,
        type_registry,
        type_import,
    );
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
    type_registry: Option<&TypeRegistry>,
    type_import: Option<String>,
) -> RS<()> {
    let code = mudu_sys::fs::sync::sync_read_to_string(input.as_ref())?;
    let procedures = discover_procedures(&code, type_registry)?;
    if procedures.is_empty() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "no AssemblyScript procedure marked with /**mudu-proc*/ found"
        ));
    }

    let adapter_source = render_adapter_source(
        input.as_ref(),
        output.as_ref(),
        &procedures,
        type_import.as_deref(),
    )?;
    mudu_sys::fs::sync::sync_write(output.as_ref(), adapter_source.as_bytes())?;

    let output_path = output.as_ref();
    let mut wit_path = output_path.to_path_buf();
    wit_path.set_extension("wit");
    mudu_sys::fs::sync::sync_write(&wit_path, render_wit(&procedures, &module_name).as_bytes())?;

    let desc_path = if let Some(desc_file) = opt_output_desc_file {
        let proc_desc_list = gen_procedure_desc_list(&module_name, &procedures);
        write_package_desc(&module_name, proc_desc_list, &desc_file)?;
        Some(desc_file)
    } else {
        None
    };

    if verbose {
        if let Some(desc_path) = desc_path {
            println!(
                "Successfully transpiled AssemblyScript, \nwrote {},\n{}, \n{}\n",
                output_path.display(),
                wit_path.display(),
                desc_path
            );
        } else {
            println!(
                "Successfully transpiled AssemblyScript, \nwrote {},\n{}\n",
                output_path.display(),
                wit_path.display()
            );
        }
    }
    Ok(())
}
