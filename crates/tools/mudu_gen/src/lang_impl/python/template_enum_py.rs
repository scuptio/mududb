//! Askama template data for a Python enum.

use crate::lang_impl::lang::enum_info::EnumInfo;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::python::py_codec::py_comments;
use crate::lang_impl::python::template_record_py::indent_comments;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_snake_case, to_snake_case_upper};
use mudu_binding::universal::uni_def::UniEnumDef;

/// Precomputed Python rendering data for one enum case.
pub struct EnumCasePy {
    /// `#`-style doc comments (empty when undocumented).
    pub comments: String,
    /// UPPER_SNAKE enum member name.
    pub member_name: String,
    /// Numeric discriminant.
    pub number: u32,
}

/// Askama template for a Python enum.
#[derive(Template)]
#[template(path = "python/enum.py.jinja", escape = "none")]
pub struct TemplateEnumPy {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// `#`-style doc comments (empty when undocumented).
    pub comments: String,
    /// Pascal-case enum name.
    pub enum_name: String,
    /// Snake-case enum name (the conversion-function prefix).
    pub enum_snake: String,
    /// Normalized enum cases.
    pub cases: Vec<EnumCasePy>,
}

impl TemplateEnumPy {
    /// Build the template from a WIT enum definition.
    pub fn from(enum_def: UniEnumDef, cfg: CodegenCfg) -> RS<Self> {
        let info = EnumInfo::from(enum_def, LangKind::Python)?;
        let cases = info
            .enum_cases
            .iter()
            .map(|case| EnumCasePy {
                comments: indent_comments(&py_comments(&case.ec_comments), 4),
                member_name: to_snake_case_upper(&case.ec_name),
                number: case.ec_number,
            })
            .collect();
        Ok(Self {
            cfg,
            comments: py_comments(&info.enum_comments),
            enum_snake: to_snake_case(&info.enum_name),
            enum_name: info.enum_name,
            cases,
        })
    }
}
