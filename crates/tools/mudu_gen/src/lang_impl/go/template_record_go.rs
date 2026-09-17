//! Askama template data for a Go record.

use crate::lang_impl::go::go_codec::{
    GO_RECORD_BIN, go_comments, go_from_value, go_to_value, go_type_name, go_variant_default,
    indent_comments,
};
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_def::UniRecordDef;

/// Precomputed Go rendering data for one record field.
pub struct RecordFieldGo {
    /// `//`-style doc comments, indented for the struct body.
    pub comments: String,
    /// Field name padded inside its comment-free run (gofmt alignment).
    pub name_padded: String,
    /// 1-based field number (the wire map key).
    pub number: u32,
    /// Go field type.
    pub type_name: String,
    /// Decoder default-seeding statement for variant-typed fields (empty for
    /// every other type, whose Go zero value already matches the wire
    /// default).
    pub default_init: String,
    /// Pre-rendered encode statements assigning the wire value to `_d[N]`.
    pub to_value_stmts: String,
    /// Pre-rendered decode statements assigning the typed value from `_val`.
    pub from_value_stmts: String,
}

/// Askama template for a Go record.
#[derive(Template)]
#[template(path = "go/record.go.jinja", escape = "none")]
pub struct TemplateRecordGo {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// `//`-style doc comments (empty when undocumented).
    pub comments: String,
    /// Pascal-case record name.
    pub record_name: String,
    /// Per-field rendering data, parallel to the WIT declaration order.
    pub fields: Vec<RecordFieldGo>,
}

impl TemplateRecordGo {
    /// Build the template from a WIT record definition.
    pub fn from(record_def: UniRecordDef, cfg: CodegenCfg) -> RS<Self> {
        let record_name = to_pascal_case(&record_def.record_name);
        // One temp counter for the whole encode function and one for the
        // decode function keeps every emitted temporary unique per function.
        let mut enc_tmp = 0;
        let mut dec_tmp = 0;
        let names: Vec<String> = record_def
            .record_fields
            .iter()
            .map(|field| to_pascal_case(&field.rf_name))
            .collect();
        let has_comments: Vec<bool> = record_def
            .record_fields
            .iter()
            .map(|field| !field.rf_comments.is_empty())
            .collect();
        let padded = crate::lang_impl::go::go_codec::pad_ident_runs(&has_comments, &names);
        let mut fields = Vec::with_capacity(record_def.record_fields.len());
        for (i, field) in record_def.record_fields.iter().enumerate() {
            // The WIT parser assigns `rf_number` after parsing; fall back to
            // the positional number for hand-built definitions (mirrors
            // `record_fields.rs`).
            let number = if field.rf_number == 0 {
                (i + 1) as u32
            } else {
                field.rf_number
            };
            let name = names[i].clone();
            let default_init = match go_variant_default(&field.rf_type, &cfg.type_kinds)? {
                Some(expr) => format!("x.{name} = {expr}"),
                None => String::new(),
            };
            fields.push(RecordFieldGo {
                comments: indent_comments(&go_comments(&field.rf_comments)),
                name_padded: padded[i].clone(),
                type_name: go_type_name(&field.rf_type)?,
                default_init,
                to_value_stmts: go_to_value(
                    &field.rf_type,
                    &format!("x.{name}"),
                    &format!("_d[{number}]"),
                    &mut enc_tmp,
                    "\t",
                    GO_RECORD_BIN,
                )?,
                from_value_stmts: go_from_value(
                    &field.rf_type,
                    "_val",
                    &format!("x.{name}"),
                    &mut dec_tmp,
                    "\t\t\t",
                    "return x, err",
                )?,
                number,
            });
        }
        Ok(Self {
            cfg,
            comments: go_comments(&record_def.record_comments),
            record_name,
            fields,
        })
    }
}
