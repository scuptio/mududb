use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::variant_info::VariantInfo;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::UniVariantDef;

#[derive(Template)]
#[template(path = "csharp/variant.cs.jinja", escape = "none")]
pub struct TemplateVariantCS {
    #[allow(unused)]
    pub cfg:CodegenCfg,
    pub variant: VariantInfo,
}

impl TemplateVariantCS {
    pub fn from(variant_def: UniVariantDef, cfg:CodegenCfg) -> RS<TemplateVariantCS> {
        Ok(TemplateVariantCS {
            cfg,
            variant: VariantInfo::from(variant_def, LangKind::CSharp)?,
        })
    }
}