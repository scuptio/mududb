//! Askama template data for a Go variant.

use crate::lang_impl::go::go_codec::{
    GO_RECORD_BIN, go_comments, go_from_value, go_local_ident, go_to_value, go_type_name,
};
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_def::UniVariantDef;

/// Precomputed Go rendering data for one variant case.
pub struct VariantCaseGo {
    /// `//`-style doc comments (empty when undocumented).
    pub comments: String,
    /// Case struct name (`UniDataTypeScalar`).
    pub struct_name: String,
    /// `Kind` const name (`UniDataTypeKindScalar`).
    pub kind_const: String,
    /// `Kind` const name padded across the const block (gofmt alignment).
    pub kind_const_padded: String,
    /// 0-based wire tag.
    pub number: u32,
    /// Whether the case carries a payload.
    pub has_inner: bool,
    /// Payload Go type (empty for payload-less cases).
    pub inner_type: String,
    /// Pre-rendered encode statements converting `_x.Inner` to the wire
    /// value `_p` (empty for payload-less cases).
    pub to_value_stmts: String,
    /// Pre-rendered decode statements converting the payload slot `_s[1]`
    /// into the typed `_p` (empty for payload-less cases).
    pub from_value_stmts: String,
}

/// Askama template for a Go variant.
#[derive(Template)]
#[template(path = "go/variant.go.jinja", escape = "none")]
pub struct TemplateVariantGo {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// `//`-style doc comments (empty when undocumented).
    pub comments: String,
    /// Pascal-case variant (interface) name.
    pub variant_name: String,
    /// Camel-case variant name (the sealed method prefix).
    pub variant_camel: String,
    /// Per-case rendering data, parallel to the WIT declaration order.
    pub cases: Vec<VariantCaseGo>,
    /// Struct name of the first case (the proto3 default instance).
    pub first_case_struct: String,
    /// Whether any case carries a payload (the type-switch guard variable is
    /// only emitted then, so payload-less-only variants have no unused
    /// binding).
    pub has_payload_case: bool,
}

impl TemplateVariantGo {
    /// Build the template from a WIT variant definition.
    pub fn from(variant_def: UniVariantDef, cfg: CodegenCfg) -> RS<Self> {
        let variant_name = to_pascal_case(&variant_def.variant_name);
        let mut enc_tmp = 0;
        let mut dec_tmp = 0;
        // The Kind const block carries no per-case comments, so all members
        // form one gofmt alignment run.
        let kind_names: Vec<String> = variant_def
            .variant_cases
            .iter()
            .map(|case| format!("{variant_name}Kind{}", to_pascal_case(&case.vc_case_name)))
            .collect();
        let kind_padded = crate::lang_impl::go::go_codec::pad_ident_runs(
            &vec![false; kind_names.len()],
            &kind_names,
        );
        let mut cases = Vec::with_capacity(variant_def.variant_cases.len());
        for (i, case) in variant_def.variant_cases.iter().enumerate() {
            let case_pascal = to_pascal_case(&case.vc_case_name);
            let has_inner = case.vc_case_type.is_some();
            let (inner_type, to_value_stmts, from_value_stmts) = match &case.vc_case_type {
                Some(ty) => {
                    let inner_type = go_type_name(ty)?;
                    // The emitters render plain assignments to the `_p`
                    // payload slot, so the slot is declared here: an `any`
                    // slot on encode, the typed payload on decode.
                    let to_value_stmts = format!(
                        "\t\tvar _p any\n{}",
                        go_to_value(ty, "_x.Inner", "_p", &mut enc_tmp, "\t\t", GO_RECORD_BIN)?
                    );
                    let from_value_stmts = format!(
                        "\t\tvar _p {inner_type}\n{}",
                        go_from_value(ty, "_s[1]", "_p", &mut dec_tmp, "\t\t", "return nil, err")?
                    );
                    (inner_type, to_value_stmts, from_value_stmts)
                }
                None => (String::new(), String::new(), String::new()),
            };
            cases.push(VariantCaseGo {
                comments: go_comments(&case.vc_comments),
                struct_name: format!("{variant_name}{case_pascal}"),
                kind_const: kind_names[i].clone(),
                kind_const_padded: kind_padded[i].clone(),
                number: i as u32,
                has_inner,
                inner_type,
                to_value_stmts,
                from_value_stmts,
            });
        }
        let has_payload_case = cases.iter().any(|case| case.has_inner);
        let first_case_struct = cases
            .first()
            .map(|case| case.struct_name.clone())
            .unwrap_or_default();
        Ok(Self {
            cfg,
            comments: go_comments(&variant_def.variant_comments),
            variant_camel: go_local_ident(&variant_def.variant_name),
            variant_name,
            cases,
            first_case_struct,
            has_payload_case,
        })
    }
}
