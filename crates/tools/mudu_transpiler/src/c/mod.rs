//! C front-end for the Mudu transpiler.
//!
//! This module discovers `// mudu-proc (name: type, ...) -> type` annotated
//! functions in C source and emits the byte-pipe adapter translation unit
//! plus the procedure world WIT: the finished freestanding component imports
//! `mududb:api/system` directly and exports one root-level `mp2-<kebab>`
//! `func(param: list<u8>) -> list<u8>` per procedure (the same architecture
//! as the AssemblyScript guest).

mod desc;
mod parser;
mod procedure;
mod render;

// tree-sitter grammars are implemented in C and call foreign functions that Miri
// does not support, so skip these parser tests under Miri.
#[cfg(all(test, not(miri)))]
mod tests;

use crate::c::desc::gen_procedure_desc_list;
use crate::c::parser::discover_procedures;
use crate::c::render::render_adapter_source;
use crate::common::desc::write_package_desc;
use crate::common::type_registry::TypeRegistry;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::path::Path;

/// Transpile C source code to Mudu procedure adapter artifacts.
///
/// The primary output is a C adapter translation unit with the generated
/// `mp2_<snake>` byte-pipe export wrappers (plus their arity/kind check
/// preambles). The sibling `.wit` file carries the procedure world (`import
/// mududb:api/system` plus one root-level `mp2-<kebab>` byte-pipe export per
/// procedure).
///
/// `type_registry` carries the user-defined types loaded from `--type-wit`:
/// annotation type names that miss the scalar table resolve against it
/// (record/enum parameters and returns, `option<T>` parameters).
/// `type_import` (`--type-import`) is the include specifier of the project's
/// mgen-generated types header; it is required when a procedure signature
/// references a registry type, and the generated adapter includes it.
pub fn transpile_c<I: AsRef<Path>, O: AsRef<Path>>(
    input: I,
    output: O,
    module_name: String,
    verbose: bool,
    opt_output_desc_file: Option<String>,
    type_registry: Option<&TypeRegistry>,
    type_import: Option<String>,
) -> i32 {
    let r = _transpile_c(
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

fn _transpile_c<I: AsRef<Path>, O: AsRef<Path>>(
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
            "no C procedure marked with // mudu-proc found"
        ));
    }

    let adapter_source = render_adapter_source(&procedures, type_import.as_deref())?;
    mudu_sys::fs::sync::sync_write(output.as_ref(), adapter_source.as_bytes())?;

    let output_path = output.as_ref();
    let mut wit_path = output_path.to_path_buf();
    wit_path.set_extension("wit");
    let proc_names = procedures
        .iter()
        .map(|procedure| procedure.name.clone())
        .collect::<Vec<_>>();
    mudu_sys::fs::sync::sync_write(
        &wit_path,
        crate::common::wit::render_wit(&proc_names, &module_name, &[]).as_bytes(),
    )?;

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
                "Successfully transpiled C, \nwrote {},\n{}, \n{}\n",
                output_path.display(),
                wit_path.display(),
                desc_path
            );
        } else {
            println!(
                "Successfully transpiled C, \nwrote {},\n{}\n",
                output_path.display(),
                wit_path.display()
            );
        }
    }
    Ok(())
}
