//! Generate message source files from WIT definitions.

use crate::lang_impl::lang::lang_kind::LangKind;
use crate::src_gen::code_gen::CodeGen;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

fn format_rust_source(src_code: &str) -> RS<String> {
    let syntax = syn::parse_file(src_code)
        .map_err(|e| mudu_error!(ErrorCode::FmtWrite, "parse source code error", e))?;
    Ok(prettyplease::unparse(&syntax))
}

/// Generate message source files from WIT inputs.
pub fn gen_message<I: AsRef<Path>, O: AsRef<Path>>(
    input_path: I,
    output_path: O,
    language: String,
    namespace: Option<String>,
) -> RS<()> {
    let lang = LangKind::from_name(language.as_str()).map_or_else(
        || Err(mudu_error!(ErrorCode::InvalidArgument, "lang unknown")),
        Ok,
    )?;
    if mudu_sys::fs::sync::sync_metadata(input_path.as_ref())?.is_dir() {
        for dir_entry in mudu_sys::fs::sync::sync_read_dir_entries(input_path.as_ref())? {
            if dir_entry.file_type()?.is_file()
                && dir_entry.path().extension() == Some(OsStr::new("wit"))
            {
                _gen_message(
                    dir_entry.path(),
                    output_path.as_ref(),
                    lang,
                    namespace.clone(),
                    true,
                )?
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
    namespace: Option<String>,
    is_input_a_dir: bool,
) -> RS<()> {
    let str = mudu_sys::fs::sync::sync_read_to_string(input_path.as_ref())?;
    let mut src_code =
        CodeGen::generate_message_code_from_wit(&str, lang_kind.to_str(), namespace)?;
    if lang_kind == LangKind::Rust {
        src_code = format_rust_source(&src_code)?;
    }
    let output_path_buf = if is_input_a_dir {
        if !mudu_sys::fs::sync::sync_path_exists(output_path.as_ref()) {
            mudu_sys::fs::sync::sync_create_dir_all(&output_path)?;
        }
        let stem = input_path.as_ref().file_stem().map_or_else(
            || {
                Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    "get file stem error"
                ))
            },
            |e| {
                e.to_str().map_or_else(
                    || Err(mudu_error!(ErrorCode::InvalidUtf8, "get file stem error")),
                    |s| Ok(s.to_string()),
                )
            },
        )?;
        let stem = if lang_kind == LangKind::Rust {
            to_snake_case(&stem)
        } else {
            to_pascal_case(&stem)
        };
        PathBuf::from(output_path.as_ref()).join(format!("{}.{}", stem, lang_kind.extension()))
    } else {
        let parent = output_path.as_ref().parent().map_or_else(
            || Err(mudu_error!(ErrorCode::InvalidArgument, "get parent error")),
            |p| Ok(p.to_path_buf()),
        )?;
        if !mudu_sys::fs::sync::sync_path_exists(&parent) {
            mudu_sys::fs::sync::sync_create_dir_all(&parent)?;
        }
        PathBuf::from(output_path.as_ref())
    };
    mudu_sys::fs::sync::sync_write(&output_path_buf, src_code)?;
    Ok(())
}
