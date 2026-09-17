//! Askama template data for a Go enum.

use crate::lang_impl::go::go_codec::{go_comments, indent_comments};
use crate::lang_impl::lang::enum_info::EnumInfo;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::UniEnumDef;

/// Precomputed Go rendering data for one enum case.
pub struct EnumCaseGo {
    /// `//`-style doc comments, indented for the const block.
    pub comments: String,
    /// Exported enum member name (`UniScalarTimestampTz`).
    pub member_name: String,
    /// Member name padded inside its comment-free run (gofmt alignment).
    pub member_name_padded: String,
    /// Numeric discriminant.
    pub number: u32,
}

/// Askama template for a Go enum.
#[derive(Template)]
#[template(path = "go/enum.go.jinja", escape = "none")]
pub struct TemplateEnumGo {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// `//`-style doc comments (empty when undocumented).
    pub comments: String,
    /// Pascal-case enum name.
    pub enum_name: String,
    /// Normalized enum cases.
    pub cases: Vec<EnumCaseGo>,
    /// Comma-separated member names (the FromValue accept list).
    pub member_list: String,
}

impl TemplateEnumGo {
    /// Build the template from a WIT enum definition.
    pub fn from(enum_def: UniEnumDef, cfg: CodegenCfg) -> RS<Self> {
        let info = EnumInfo::from(enum_def, LangKind::Go)?;
        let names: Vec<String> = info
            .enum_cases
            .iter()
            .map(|case| format!("{}{}", info.enum_name, case.ec_name))
            .collect();
        let has_comments: Vec<bool> = info
            .enum_cases
            .iter()
            .map(|case| !case.ec_comments.is_empty())
            .collect();
        let padded = crate::lang_impl::go::go_codec::pad_ident_runs(&has_comments, &names);
        let cases: Vec<EnumCaseGo> = info
            .enum_cases
            .iter()
            .enumerate()
            .map(|(i, case)| EnumCaseGo {
                comments: indent_comments(&go_comments(&case.ec_comments)),
                member_name: names[i].clone(),
                member_name_padded: padded[i].clone(),
                number: case.ec_number,
            })
            .collect();
        let member_list = cases
            .iter()
            .map(|case| case.member_name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        Ok(Self {
            cfg,
            comments: go_comments(&info.enum_comments),
            enum_name: info.enum_name,
            cases,
            member_list,
        })
    }
}
