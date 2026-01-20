use crate::lang_impl::lang::lang_kind::LangKind;
use crate::src_gen::code_gen::CodeGen;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use rust_format::{Formatter, RustFmt};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

pub fn gen_message<I: AsRef<Path>, O: AsRef<Path>>(
    input_path: I,
    output_path: O,
    language: String,
    namespace: Option<String>,
) -> RS<()> {
    let lang = LangKind::from_str(language.as_str())
        .map_or_else(|| { Err(m_error!(EC::NoneErr, "lang unknown")) }, |l| Ok(l))?;
    if input_path.as_ref().is_dir() {
        for r_dir_entry in fs::read_dir(input_path.as_ref())
            .map_err(|e| m_error!(EC::IOErr, "read dir error", e))? {
            let dir_entry = r_dir_entry.map_err(|e| m_error!(EC::IOErr, "read dir error", e))?;
            if dir_entry.path().is_file() && dir_entry.path().extension() == Some(OsStr::new("wit")) {
                _gen_message(dir_entry.path(), output_path.as_ref(), lang, namespace.clone(),true)?
            }
        }
    } else {
        _gen_message(input_path, output_path, lang, namespace, false)?;
    }
    Ok(())
}


fn _gen_message<I: AsRef<Path>, O: AsRef<Path>>(
    input_path: I,
    output_path: O,
    lang_kind: LangKind,
    namespace:Option<String>,
    is_input_a_dir:bool,
) -> RS<()> {
    let str = fs::read_to_string(input_path.as_ref())
    .map_err(|e| m_error!(EC::IOErr, "read input wit error", e))?;
    let mut src_code = CodeGen::generate_message_code_from_wit(&str, lang_kind.to_str(), namespace)?;
    if lang_kind == LangKind::Rust {
        src_code = RustFmt::default().format_str(src_code)
            .map_err(
                |e| m_error!(EC::IOErr, "fmt source code error", e)
            )?;
    }
    let output_path_buf = if is_input_a_dir {
        if !output_path.as_ref().exists() {
            fs::create_dir_all(&output_path)
                .map_err(|e| m_error!(EC::IOErr, "create dir error", e))?;
        }
        let stem = input_path.as_ref().file_stem()
            .map_or_else(
                || { Err(m_error!(EC::NoneErr, "get file stem error")) },
                |e| e.to_str()
                    .map_or_else(|| { Err(m_error!(EC::NoneErr, "get file stem error")) },
                                 |s| { Ok(s.to_string()) }))?;
        let stem = if lang_kind == LangKind::Rust {
            to_snake_case(&stem)
        } else {
            to_pascal_case(&stem)
        };
        PathBuf::from(output_path.as_ref()).join(format!("{}.{}", stem, lang_kind.extension()))
    } else {
        let parent = output_path.as_ref().parent()
            .map_or_else(|| { Err(m_error!(EC::NoneErr, "get parent error")) },
                         |p| { Ok(p.to_path_buf()) })?;
        if !parent.exists() {
            fs::create_dir_all(&parent)
                .map_err(|e| m_error!(EC::IOErr, "create dir error", e))?;
        }
        PathBuf::from(output_path.as_ref())
    };
    fs::write(&output_path_buf, src_code).unwrap();
    Ok(())
}