use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::variant_info::VariantInfo;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::UniVariantDef;

/// Askama template for a Rust variant.
#[derive(Template)]
#[template(path = "rust/variant.rs.jinja", escape = "none")]
pub struct TemplateVariantRS {
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Normalized variant metadata.
    pub variant: VariantInfo,
}

impl TemplateVariantRS {
    /// Build the template from a WIT variant definition.
    pub fn from(variant_def: UniVariantDef, cfg: CodegenCfg) -> RS<TemplateVariantRS> {
        Ok(TemplateVariantRS {
            cfg,
            variant: VariantInfo::from(variant_def, LangKind::Rust)?,
        })
    }
}
