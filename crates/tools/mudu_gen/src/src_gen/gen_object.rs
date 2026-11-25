//! Generate object source files from DDL SQL.

use crate::src_gen::code_gen::{CodeGen, GenResult};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_snake_case;
use mudu::utils::json::to_json_str;

use std::path::PathBuf;

/// Generate object source files from DDL SQL files.
pub fn gen_rust(
    input_ddl_path: Vec<String>,
    output_dir_path: String,
    type_desc: Option<String>,
    language: String,
) -> RS<()> {
    _gen_from_ddl_sql(input_ddl_path, output_dir_path, type_desc, language)?;
    Ok(())
}

/// Generate a WIT module from DDL SQL files (the wit backend).
///
/// Concatenates the DDL files and projects them onto a single `package` /
/// `interface` WIT module. Length/precision are dropped (pipeline design §4).
pub fn gen_wit(
    input_ddl_path: Vec<String>,
    package: String,
    interface: String,
    output_wit_path: String,
) -> RS<()> {
    let mut ddl = String::new();
    for file in input_ddl_path {
        ddl.push_str(&mudu_sys::fs::sync::sync_read_to_string(file)?);
        ddl.push('\n');
    }
    let wit = CodeGen::generate_wit_from_ddl(&ddl, &package, &interface)?;
    mudu_sys::fs::sync::sync_write(output_wit_path, wit)?;
    Ok(())
}

// read from DDL SQL file, and generate source code
fn _gen_from_ddl_sql(
    input_ddl_path: Vec<String>,
    output_source_path: String,
    opt_ty_desc_path: Option<String>,
    lang: String,
) -> RS<()> {
    let out_path_buf = PathBuf::from(output_source_path);
    if !mudu_sys::fs::sync::sync_path_exists(&out_path_buf) {
        mudu_sys::fs::sync::sync_create_dir_all(&out_path_buf)?;
    }
    if !mudu_sys::fs::sync::sync_metadata(&out_path_buf)?.is_dir() {
        return Err(mudu_error!(
            ErrorCode::NotADirectory,
            format!("Output directory {:?} is not a directory", out_path_buf)
        ));
    }
    let mut gen_result = GenResult::default();
    for file in input_ddl_path {
        let sql_text = mudu_sys::fs::sync::sync_read_to_string(file)?;
        let gr = CodeGen::generate_object_code_from_ddl_sql(
            &sql_text,
            &lang,
            opt_ty_desc_path.is_some(),
        )?;
        gen_result.extend(gr)
    }

    let extension = CodeGen::extension_of_lang(lang.as_str())?;
    for (name, source) in gen_result.source_code {
        let stem = to_snake_case(&name);
        let file = format!("{}.{}", stem, extension);
        let file_path = out_path_buf.join(file);
        mudu_sys::fs::sync::sync_write(file_path, source)?;
    }
    if let Some(ty_desc_path) = opt_ty_desc_path {
        let content = to_json_str(&gen_result.used_defined_struct_type)?;
        mudu_sys::fs::sync::sync_write(ty_desc_path, &content)?;
    }
    Ok(())
}
