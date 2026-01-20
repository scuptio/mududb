use crate::rust::parse_context::ParseContext;
use crate::rust::rust_parser::RustParser;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::json::{from_json_str, to_json_str};
use mudu_contract::procedure::package_desc::PackageDesc;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use mudu_binding::universal::uni_type_desc::UniTypeDesc;

mod parse_context;
#[allow(unused)]
mod rust_parser;
mod template_proc;
#[allow(unused)]
mod ts_const;
mod rust_type;
mod function;


/// Transpile Rust source code to target language
pub fn transpile_rust<I: AsRef<Path>, O: AsRef<Path>>(
    input: I,
    output: O,
    module_name: String,
    verbose: bool,
    enable_async: bool,
    src_mod: Option<String>,
    dst_mod: Option<String>,
    output_desc_file: Option<String>,
    opt_custom_type_def_file:Option<String>,
) -> i32 {
    let r = _transpile_rust(
        input,
        output,
        module_name,
        verbose,
        enable_async,
        src_mod,
        dst_mod,
        output_desc_file,
        opt_custom_type_def_file
    );
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("transpile error: {}", e);
            e.ec().to_u32() as i32
        }
    }
}

pub fn _transpile_rust<I: AsRef<Path>, O: AsRef<Path>>(
    input: I,
    output: O,
    module_name: String,
    verbose: bool,
    enable_async: bool,
    src_mod: Option<String>,
    dst_mod: Option<String>,
    opt_output_desc_file: Option<String>,
    opt_custom_type_def_file:Option<String>,
) -> RS<()> {
    // Read input file
    let code = fs::read_to_string(input)
        .map_err(|e| m_error!(EC::IOErr, "read rust source code error", e))?;
    let mut context = ParseContext::new(code, src_mod, dst_mod);
    RustParser::parse(&mut context)?;
    if enable_async {
        context.tran_to_async();
    }

    // Placeholder for actual transpilation logic
    let transpiled_code = context.render_source(module_name.clone(), enable_async)?;

    // Write output file
    fs::write(&output, transpiled_code)
        .map_err(|e| m_error!(EC::IOErr, "write transpiled rust source code error", e))?;

    if let Some(desc_files) = opt_output_desc_file {
        let custom_types = if let Some(type_desc_file) = opt_custom_type_def_file{
            let text = fs::read_to_string(type_desc_file)
                .map_err(|e|m_error!(EC::IOErr, "read type description file error", e))?;
            let map = from_json_str::<UniTypeDesc>(&text)?;
            map
        } else {
            Default::default()
        };
        let proc_desc_list = context.gen_procedure_desc_list(&module_name, &custom_types)?;
        let modules = HashMap::from_iter(vec![(module_name.clone(), proc_desc_list)]);
        let package_desc = PackageDesc::new(modules);
        let toml_str = to_json_str(&package_desc)?;
        fs::write(&desc_files, toml_str)
            .map_err(|e| m_error!(EC::IOErr, "write description files error", e))?;
    }
    if verbose {
        println!(
            "Successfully transpiled Rust, write to {}",
            output.as_ref().display()
        );
    }
    Ok(())
}
