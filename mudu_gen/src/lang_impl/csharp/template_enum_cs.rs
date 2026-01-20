use crate::lang_impl::lang::enum_info::EnumInfo;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::UniEnumDef;

#[derive(Template)]
#[template(path = "csharp/enum.cs.jinja", escape = "none")]
pub struct TemplateEnumCS {
    #[allow(unused)]
    pub cfg:CodegenCfg,
    pub enum_def:EnumInfo,
}

impl TemplateEnumCS {
    pub fn from(enum_def: UniEnumDef, cfg:CodegenCfg) -> RS<TemplateEnumCS> {
        Ok(Self {
            cfg,
            enum_def: EnumInfo::from(enum_def, LangKind::CSharp)?,
        })
    }
}
