//! Generate message source files from WIT definitions.

use crate::lang_impl::lang::lang_kind::LangKind;
use crate::src_gen::code_gen::CodeGen;
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::schema_desc::build_schema_desc;
use crate::src_gen::wit_def::WitDef;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu::utils::json::to_json_str;
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
    gen_message_with_cfg(
        input_path,
        output_path,
        language,
        namespace,
        CodegenCfg::new(),
        None,
    )
}

/// Generate message source files from WIT inputs with an explicit generation
/// configuration (e.g. MSSP func-codec generation).
///
/// When `type_desc` is `Some`, a `syscall_schema` descriptor JSON
/// ([`mudu_binding::universal::uni_schema_desc::UniSchemaDesc`]) aggregating
/// every parsed WIT file is written to that path after all source files are
/// generated.
///
/// For the Go back-end, a `wire.go` carrying the hand-written helper runtime
/// (the checked-conversion and map/list expectation functions the generated
/// codecs reference) is emitted next to the generated files; its package
/// clause follows the same namespace resolution as the generated code (see
/// [`crate::lang_impl::go::go_codec::go_package_name`]).
pub fn gen_message_with_cfg<I: AsRef<Path>, O: AsRef<Path>>(
    input_path: I,
    output_path: O,
    language: String,
    namespace: Option<String>,
    cfg: CodegenCfg,
    type_desc: Option<String>,
) -> RS<()> {
    let lang = LangKind::from_name(language.as_str()).map_or_else(
        || Err(mudu_error!(ErrorCode::InvalidArgument, "lang unknown")),
        Ok,
    )?;
    if mudu_sys::fs::sync::sync_metadata(input_path.as_ref())?.is_dir() {
        // Pre-parse every WIT file so the AssemblyScript type-kind registry
        // covers cross-file identifier references (e.g. enum defaults).
        let mut cfg = cfg;
        let mut entries = Vec::new();
        for dir_entry in mudu_sys::fs::sync::sync_read_dir_entries(input_path.as_ref())? {
            if dir_entry.file_type()?.is_file()
                && dir_entry.path().extension() == Some(OsStr::new("wit"))
            {
                entries.push(dir_entry.path());
            }
        }
        let parser = crate::src_gen::wit_parser::WitParser::new();
        let mut wit_defs: Vec<WitDef> = Vec::new();
        for path in &entries {
            let text = mudu_sys::fs::sync::sync_read_to_string(path)?;
            let wit_dat = parser.parse_text(&text)?;
            for enum_def in &wit_dat.enums {
                cfg.type_kinds
                    .insert(to_pascal_case(&enum_def.enum_name), "enum".to_string());
            }
            for variant_def in &wit_dat.variants {
                cfg.type_kinds.insert(
                    to_pascal_case(&variant_def.variant_name),
                    "variant".to_string(),
                );
            }
            for record_def in &wit_dat.records {
                cfg.type_kinds.insert(
                    to_pascal_case(&record_def.record_name),
                    "record".to_string(),
                );
            }
            wit_defs.push(wit_dat);
        }
        let mut stems: Vec<String> = Vec::new();
        for path in entries {
            let stem = file_stem(&path)?;
            stems.push(stem);
            _gen_message(
                path,
                output_path.as_ref(),
                lang,
                namespace.clone(),
                cfg.clone(),
                true,
            )?
        }
        if !stems.is_empty() {
            write_module_index(output_path.as_ref(), lang, &stems)?;
        }
        if lang == LangKind::Go {
            emit_go_wire_helpers(output_path.as_ref(), &resolve_go_package(&namespace))?;
        }
        if let Some(desc_path) = type_desc {
            write_schema_desc(&wit_defs, desc_path)?;
        }
    } else {
        _gen_message(
            input_path.as_ref(),
            output_path.as_ref(),
            lang,
            namespace.clone(),
            cfg,
            false,
        )?;
        if lang == LangKind::Go {
            let output_dir = output_path
                .as_ref()
                .parent()
                .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "get parent error"))?;
            emit_go_wire_helpers(output_dir, &resolve_go_package(&namespace))?;
        }
        if let Some(desc_path) = type_desc {
            let text = mudu_sys::fs::sync::sync_read_to_string(input_path.as_ref())?;
            let wit_dat = crate::src_gen::wit_parser::WitParser::new().parse_text(&text)?;
            write_schema_desc(&[wit_dat], desc_path)?;
        }
    }
    Ok(())
}

/// The Go wire-helper runtime (`crates/sdk/bindings/go/types/wire.go`),
/// embedded so project generations get a byte-identical copy of the
/// hand-written original (single source of truth).
const GO_WIRE_HELPERS: &str = include_str!("../../../../sdk/bindings/go/types/wire.go");

/// Resolve the Go package clause for one message generation: the
/// `--namespace` flag when given, else the canonical `types` (see
/// [`crate::lang_impl::go::go_codec::go_package_name`]).
fn resolve_go_package(namespace: &Option<String>) -> String {
    crate::lang_impl::go::go_codec::go_package_name(namespace.as_deref().unwrap_or(""))
}

/// Emit the Go wire-helper runtime next to the generated files with the
/// package clause rewritten to `package_name` (a no-op rewrite for the
/// canonical `types` package keeps the binding regeneration byte-identical).
fn emit_go_wire_helpers(output_dir: &Path, package_name: &str) -> RS<()> {
    if !mudu_sys::fs::sync::sync_path_exists(output_dir) {
        mudu_sys::fs::sync::sync_create_dir_all(output_dir)?;
    }
    let source = GO_WIRE_HELPERS.replacen(
        "\npackage types\n",
        &format!("\npackage {package_name}\n"),
        1,
    );
    mudu_sys::fs::sync::sync_write(output_dir.join("wire.go"), source)?;
    Ok(())
}

fn write_schema_desc(wit_defs: &[WitDef], desc_path: String) -> RS<()> {
    let desc = build_schema_desc(wit_defs);
    let content = to_json_str(&desc)?;
    mudu_sys::fs::sync::sync_write(desc_path, &content)?;
    Ok(())
}

fn file_stem(path: &Path) -> RS<String> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidUtf8, "get file stem error"))
}

fn write_module_index(output_dir: &Path, lang: LangKind, stems: &[String]) -> RS<()> {
    if !mudu_sys::fs::sync::sync_path_exists(output_dir) {
        mudu_sys::fs::sync::sync_create_dir_all(output_dir)?;
    }
    let index_content = match lang {
        LangKind::Rust => {
            let mut s = String::new();
            for stem in stems {
                s.push_str(&format!("pub mod {};\n", to_snake_case(stem)));
            }
            s
        }
        LangKind::CSharp => {
            // C# files already share a namespace; no per-file index is required.
            String::new()
        }
        LangKind::AssemblyScript => {
            let mut s = String::new();
            for stem in stems {
                s.push_str(&format!("export * from \"./{}\";\n", to_pascal_case(stem)));
            }
            s
        }
        LangKind::Python => {
            let mut s = String::from("# Generated by mudu_gen. Do not edit manually.\n");
            for stem in stems {
                s.push_str(&format!("from . import {}\n", to_snake_case(stem)));
            }
            s
        }
        LangKind::C | LangKind::Go => {
            // C/Go entity files are self-contained; no per-file index.
            String::new()
        }
    };
    if index_content.is_empty() {
        return Ok(());
    }
    let index_name = match lang {
        LangKind::Rust => "mod.rs",
        LangKind::CSharp => return Ok(()),
        LangKind::AssemblyScript => "index.ts",
        LangKind::Python => "__init__.py",
        LangKind::C | LangKind::Go => return Ok(()),
    };
    let index_path = output_dir.join(index_name);
    mudu_sys::fs::sync::sync_write(index_path, index_content)?;
    Ok(())
}

fn _gen_message<I: AsRef<Path>, O: AsRef<Path>>(
    input_path: I,
    output_path: O,
    lang_kind: LangKind,
    namespace: Option<String>,
    cfg: CodegenCfg,
    is_input_a_dir: bool,
) -> RS<()> {
    let str = mudu_sys::fs::sync::sync_read_to_string(input_path.as_ref())?;
    let mut cfg = cfg;
    if cfg.with_func_codec && cfg.func_module_name.is_empty() {
        // The generated func-codec containers (e.g. the C# static class) are
        // named after the input WIT file stem.
        cfg.func_module_name = to_pascal_case(&file_stem(input_path.as_ref())?);
    }
    let mut src_code =
        CodeGen::generate_message_code_from_wit_with_cfg(&str, lang_kind.to_str(), namespace, cfg)?;
    if lang_kind == LangKind::Rust {
        src_code = format_rust_source(&src_code)?;
    }
    src_code = if lang_kind == LangKind::Rust {
        // The generated wire glue is deliberately mechanical; silence the
        // style lints it intentionally trips (e.g. `&Vec` parameters mirror
        // the WIT signatures, `&Box<T>` accessors mirror the variant shapes,
        // `expect_*` accessors panic on a wrong variant by design), the
        // dead-code lint (consumers rarely use every generated stub) and the
        // unused-import lint (the blanket `mp_wire` runtime import covers
        // items a given file may not reference) so downstream crates can
        // compile it under `-D warnings`.
        format!(
            "// Generated by mudu_gen. Do not edit manually.\n\
             #![allow(\n    \
             dead_code,\n    \
             unused_imports,\n    \
             clippy::borrowed_box,\n    \
             clippy::derivable_impls,\n    \
             clippy::map_identity,\n    \
             clippy::needless_borrow,\n    \
             clippy::panic,\n    \
             clippy::ptr_arg,\n    \
             clippy::redundant_closure,\n    \
             clippy::single_match,\n    \
             clippy::type_complexity\n)]\n\
             {}",
            src_code
        )
    } else if lang_kind == LangKind::Python {
        format!(
            "# Generated by mudu_gen. Do not edit manually.\n{}",
            src_code
        )
    } else {
        format!(
            "// Generated by mudu_gen. Do not edit manually.\n{}",
            src_code
        )
    };
    let output_path_buf = if is_input_a_dir {
        if !mudu_sys::fs::sync::sync_path_exists(output_path.as_ref()) {
            mudu_sys::fs::sync::sync_create_dir_all(&output_path)?;
        }
        let stem = file_stem(input_path.as_ref())?;
        let stem = if lang_kind == LangKind::Rust || lang_kind == LangKind::Python {
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
