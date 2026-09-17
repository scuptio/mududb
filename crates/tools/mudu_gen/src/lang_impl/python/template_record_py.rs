//! Askama template data for a Python record.

use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::record_info::RecordInfo;
use crate::lang_impl::python::py_codec::py_comments;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_snake_case;
use mudu_binding::universal::uni_def::UniRecordDef;

/// Precomputed Python rendering data for one record field.
pub struct RecordFieldPy {
    /// `#`-style doc comments, indented for the dataclass body.
    pub comments: String,
    /// Snake-case, keyword-safe field name.
    pub name: String,
    /// 1-based field number (the wire map key).
    pub number: u32,
    /// Python type annotation.
    pub annotation: String,
    /// Dataclass field right-hand side producing the proto3 default.
    pub default: String,
    /// Wire-value conversion expression on `x.<name>`.
    pub to_value_expr: String,
    /// Typed-value conversion expression on the map value `_val`.
    pub from_value_expr: String,
    /// Whether the field is a WIT `option<T>`; omitted from the encoded
    /// map when None.
    pub is_option: bool,
}

/// Askama template for a Python record.
#[derive(Template)]
#[template(path = "python/record.py.jinja", escape = "none")]
pub struct TemplateRecordPy {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// `#`-style doc comments (empty when undocumented).
    pub comments: String,
    /// Pascal-case record name.
    pub record_name: String,
    /// Snake-case record name (the conversion-function prefix).
    pub record_snake: String,
    /// Per-field rendering data, parallel to the WIT declaration order.
    pub fields: Vec<RecordFieldPy>,
    /// Whether any field is a WIT `option<T>`; the encoder only emits the
    /// omit-when-absent guarded form then (option-less records keep the
    /// historical dict-literal shape).
    pub has_option_field: bool,
}

impl TemplateRecordPy {
    /// Build the template from a WIT record definition.
    pub fn from(record_def: UniRecordDef, cfg: CodegenCfg) -> RS<Self> {
        let record = RecordInfo::from(record_def, LangKind::Python, &cfg.type_kinds)?;
        let has_option_field = record.record_fields.iter().any(|f| f.rf_is_option);
        let fields = record
            .record_fields
            .iter()
            .map(|field| RecordFieldPy {
                comments: indent_comments(&py_comments(&field.rf_comments), 4),
                name: field.rf_name.clone(),
                number: field.rf_number,
                annotation: field.rf_type.clone(),
                default: field.rf_default_value.clone(),
                to_value_expr: field.rf_to_value.clone(),
                from_value_expr: field.rf_from_value.clone(),
                is_option: field.rf_is_option,
            })
            .collect();
        Ok(Self {
            cfg,
            comments: py_comments(&record.record_comments),
            record_snake: to_snake_case(&record.record_name),
            record_name: record.record_name,
            fields,
            has_option_field,
        })
    }
}

/// Indent every line of a comment block by `indent` spaces (empty input stays
/// empty).
pub fn indent_comments(comments: &str, indent: usize) -> String {
    if comments.is_empty() {
        return String::new();
    }
    let pad = " ".repeat(indent);
    comments
        .lines()
        .map(|line| format!("{pad}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
