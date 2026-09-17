//! Askama template data for a Python variant.

use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::variant_info::VariantInfo;
use crate::lang_impl::python::py_codec::py_comments;
use crate::lang_impl::python::template_record_py::indent_comments;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_snake_case;
use mudu_binding::universal::uni_def::UniVariantDef;

/// Precomputed Python rendering data for one variant case.
pub struct VariantCasePy {
    /// `#`-style doc comments, indented for the class body.
    pub comments: String,
    /// Case subclass name (`UniScalarValueU8`).
    pub class_name: String,
    /// UPPER_SNAKE `Kind` enum member name.
    pub member_name: String,
    /// 0-based wire tag.
    pub number: u32,
    /// Whether the case carries a payload.
    pub has_inner: bool,
    /// Payload type annotation (empty for payload-less cases).
    pub inner_annotation: String,
    /// Payload dataclass field right-hand side (empty for payload-less cases).
    pub inner_default: String,
    /// Wire-value conversion expression on `x.inner` (empty for
    /// payload-less cases).
    pub to_value_expr: String,
    /// Typed-value conversion expression on the payload slot `v[1]` (empty
    /// for payload-less cases).
    pub from_value_expr: String,
}

/// Askama template for a Python variant.
#[derive(Template)]
#[template(path = "python/variant.py.jinja", escape = "none")]
pub struct TemplateVariantPy {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// `#`-style doc comments (empty when undocumented).
    pub comments: String,
    /// Pascal-case variant (base class) name.
    pub variant_name: String,
    /// Snake-case variant name (the conversion-function prefix).
    pub variant_snake: String,
    /// Per-case rendering data, parallel to the WIT declaration order.
    pub cases: Vec<VariantCasePy>,
    /// Class name of the first case (the proto3 default instance).
    pub first_case_class: String,
}

impl TemplateVariantPy {
    /// Build the template from a WIT variant definition.
    pub fn from(variant_def: UniVariantDef, cfg: CodegenCfg) -> RS<Self> {
        let variant = VariantInfo::from(variant_def, LangKind::Python, &cfg.type_kinds)?;
        let mut cases = Vec::with_capacity(variant.variant_cases.len());
        for case in &variant.variant_cases {
            cases.push(VariantCasePy {
                comments: indent_comments(&py_comments(&case.vc_comments), 4),
                class_name: format!("{}{}", variant.variant_name, case.vc_case_name),
                member_name: case.vc_case_name_snake.to_uppercase(),
                number: case.vc_number,
                has_inner: case.vc_has_inner_type,
                inner_annotation: if case.vc_has_inner_type {
                    case.vc_inner_type_name.clone()
                } else {
                    String::new()
                },
                inner_default: if case.vc_has_inner_type {
                    case.vc_inner_default_value.clone()
                } else {
                    String::new()
                },
                to_value_expr: case.vc_to_value.clone(),
                from_value_expr: case.vc_from_value.clone(),
            });
        }
        let first_case_class = format!(
            "{}{}",
            variant.variant_name, variant.variant_cases[0].vc_case_name
        );
        Ok(Self {
            cfg,
            comments: py_comments(&variant.variant_comments),
            variant_snake: to_snake_case(&variant.variant_name),
            variant_name: variant.variant_name,
            cases,
            first_case_class,
        })
    }
}
