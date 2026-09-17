//! Go front-end for the Mudu transpiler.
//!
//! This module discovers `// mudu-proc` marked functions in Go source and
//! emits the byte-pipe adapter module plus the procedure world WIT: the
//! finished TinyGo wasip2 component imports `mududb:api/system` (plus the
//! WASI 0.2 interfaces of `wasi:cli/imports@0.2.0` the TinyGo runtime links
//! against) and exports one root-level `mp2-<kebab>`
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

use crate::common::desc::write_package_desc;
use crate::common::type_registry::TypeRegistry;
use crate::go::desc::gen_procedure_desc_list;
use crate::go::parser::discover_procedures;
use crate::go::render::render_adapter_source;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::path::Path;

/// The TinyGo wasip2 runtime needs the standard WASI 0.2 interfaces (stdio
/// for panic/println output, clocks, random for map seeds, ...); the Go world
/// includes them. `wasm-tools component new` prunes whatever the module does
/// not use, and mudud wires the full WASI 0.2 set (`wasmtime_wasi::p2`) at
/// instantiation time, so every included import resolves on the host.
const GO_EXTRA_WORLD_LINES: &[&str] = &["include wasi:cli/imports@0.2.0;"];

/// Transpile Go source code to Mudu procedure adapter artifacts.
///
/// The primary output is a Go `package main` adapter module wiring the
/// wit-bindgen-go `Exports` table to the discovered procedures. The sibling
/// `.wit` file carries the procedure world (`include
/// wasi:cli/imports@0.2.0;`, `import mududb:api/system`, plus one root-level
/// `mp2-<kebab>` byte-pipe export per procedure).
///
/// `type_registry` carries the user-defined types loaded from `--type-wit`;
/// a procedure signature referencing one of those types is decoded through
/// the project's mgen-generated Go types package, whose import path is
/// `type_import` (`--type-import`).
#[allow(clippy::too_many_arguments)]
pub fn transpile_go<I: AsRef<Path>, O: AsRef<Path>>(
    input: I,
    output: O,
    module_name: String,
    verbose: bool,
    opt_output_desc_file: Option<String>,
    type_registry: Option<&TypeRegistry>,
    type_import: Option<String>,
) -> i32 {
    let r = _transpile_go(
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

fn _transpile_go<I: AsRef<Path>, O: AsRef<Path>>(
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
            "no Go procedure marked with // mudu-proc found"
        ));
    }

    let adapter_source = render_adapter_source(&procedures, &module_name, type_import.as_deref())?;
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
        crate::common::wit::render_wit(&proc_names, &module_name, GO_EXTRA_WORLD_LINES).as_bytes(),
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
                "Successfully transpiled Go, \nwrote {},\n{}, \n{}\n",
                output_path.display(),
                wit_path.display(),
                desc_path
            );
        } else {
            println!(
                "Successfully transpiled Go, \nwrote {},\n{}\n",
                output_path.display(),
                wit_path.display()
            );
        }
    }
    Ok(())
}
