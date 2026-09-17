use crate::lang_impl::lang::enum_info::EnumInfo;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::UniEnumDef;

/// Askama template for a Rust enum.
#[derive(Template)]
#[template(path = "rust/enum.rs.jinja", escape = "none")]
pub struct TemplateEnumRS {
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Normalized enum metadata.
    pub enum_def: EnumInfo,
}

impl TemplateEnumRS {
    /// Build the template from a WIT enum definition.
    pub fn from(enum_def: UniEnumDef, cfg: CodegenCfg) -> RS<TemplateEnumRS> {
        Ok(Self {
            cfg,
            enum_def: EnumInfo::from(enum_def, LangKind::Rust)?,
        })
    }
}
