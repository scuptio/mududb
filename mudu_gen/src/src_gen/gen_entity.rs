use crate::src_gen::code_gen::{CodeGen, GenResult};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::case_convert::to_snake_case;
use mudu::utils::json::to_json_str;
use std::fs;
use std::fs::read_to_string;
use std::path::PathBuf;

pub fn gen_rust(
    input_ddl_path: Vec<String>,
    output_dir_path: String,
    type_desc: Option<String>,
    language:String,
) -> RS<()> {
    _gen_from_ddl_sql(input_ddl_path, output_dir_path, type_desc, language)?;
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
    if !out_path_buf.exists() {
        fs::create_dir_all(&out_path_buf)
            .unwrap_or_else(|_| panic!("Unable to create output directory {:?}", &out_path_buf));
    }
    if !out_path_buf.is_dir() {
        panic!("Output directory {:?} is not a directory", &out_path_buf);
    }
    let mut gen_result = GenResult::default();
    for file in input_ddl_path {
        let sql_text = read_to_string(file)
            .map_err(|e| m_error!(EC::IOErr, "open DDL SQL file error", e))?;
        let gr = CodeGen::generate_entity_code_from_ddl_sql(
            &sql_text, &lang, opt_ty_desc_path.is_some())?;
        gen_result.extend(gr)
    }

    let extension = CodeGen::extension_of_lang(lang.as_str())?;
    for (name, source) in gen_result.source_code {
        let stem = to_snake_case(&name);
        let file = format!("{}.{}", stem, extension);
        let file_path = out_path_buf.join(file);
        fs::write(file_path, source)
            .map_err(|e| m_error!(EC::IOErr, "write source error", e))?;
    }
    if let Some(ty_desc_path) = opt_ty_desc_path {
        let content = to_json_str(&gen_result.used_defined_record_type)?;
        fs::write(ty_desc_path, &content)
            .map_err(|e| m_error!(EC::IOErr, "write defined record type error", e))?;
    }
    Ok(())
}