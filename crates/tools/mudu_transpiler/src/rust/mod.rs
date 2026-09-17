//! Rust front-end: parse Rust source code, discover `/**mudu-proc**/`
//! procedures, and render the generated procedure wrapper.

use crate::rust::parse_context::ParseContext;
use crate::rust::rust_parser::RustParser;
use mudu::common::result::RS;
use mudu::utils::json::{from_json_str, to_json_str};
use mudu_binding::universal::uni_type_desc::UniTypeDesc;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use std::collections::HashMap;
use std::path::Path;

mod function;
mod parse_context;
// Rust parser is a work in progress; some visitor helpers are not yet wired up.
#[allow(unused)]
mod rust_parser;
mod rust_type;
mod template_proc;
#[allow(unused)]
mod ts_const;

/// Options controlling Rust-to-Mudu transpilation.
pub struct TranspileRustOptions<I, O> {
    /// Input Rust source path.
    pub input: I,
    /// Output path for the generated Rust source.
    pub output: O,
    /// Target module name for generated descriptors.
    pub module_name: String,
    /// Print progress messages.
    pub verbose: bool,
    /// Convert synchronous Mudu calls to `async`/`await`.
    pub enable_async: bool,
    /// Source module path to rewrite in `use` declarations.
    pub src_mod: Option<String>,
    /// Destination module path to use when rewriting `use` declarations.
    pub dst_mod: Option<String>,
    /// Optional path to write the JSON procedure description.
    pub output_desc_file: Option<String>,
    /// Optional path to a custom type description JSON file.
    pub custom_type_def_file: Option<String>,
}

/// Transpile Rust source code to Mudu procedure artifacts.
///
/// Returns an exit code: `0` on success, or a non-zero [`mudu::error::ErrorCode`]
/// code on failure.
pub fn transpile_rust<I: AsRef<Path>, O: AsRef<Path>>(options: TranspileRustOptions<I, O>) -> i32 {
    let r = _transpile_rust(options);
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("transpile error: {}", e);
            e.ec().to_u32() as i32
        }
    }
}

/// Internal implementation of [`transpile_rust`] that returns a typed result.
pub fn _transpile_rust<I: AsRef<Path>, O: AsRef<Path>>(
    options: TranspileRustOptions<I, O>,
) -> RS<()> {
    let TranspileRustOptions {
        input,
        output,
        module_name,
        verbose,
        enable_async,
        src_mod,
        dst_mod,
        output_desc_file,
        custom_type_def_file,
    } = options;
    // Read input file
    let code = mudu_sys::fs::sync::sync_read_to_string(input)?;
    let mut context = ParseContext::new(code, src_mod, dst_mod);
    RustParser::parse(&mut context)?;
    if enable_async {
        context.tran_to_async();
    }

    // Placeholder for actual transpilation logic
    let transpiled_code = context.render_source(module_name.clone(), enable_async)?;

    // Write output file
    mudu_sys::fs::sync::sync_write(&output, transpiled_code)?;

    if let Some(desc_files) = output_desc_file {
        let custom_types = if let Some(type_desc_file) = custom_type_def_file {
            let text = mudu_sys::fs::sync::sync_read_to_string(type_desc_file)?;
            from_json_str::<UniTypeDesc>(&text)?
        } else {
            Default::default()
        };
        let proc_desc_list = context.gen_procedure_desc_list(&module_name, &custom_types)?;
        let modules = HashMap::from_iter(vec![(module_name.clone(), proc_desc_list)]);
        let package_desc = ModProcDesc::new(modules);
        let toml_str = to_json_str(&package_desc)?;
        mudu_sys::fs::sync::sync_write(&desc_files, toml_str)?;
    }
    if verbose {
        println!(
            "Successfully transpiled Rust, write to {}",
            output.as_ref().display()
        );
    }
    Ok(())
}
