use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::table_info::TableInfo;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::UniTableDef;

#[derive(Template)]
#[template(path = "csharp/table.cs.jinja", escape = "none")]
pub struct TemplateTableCS {
    #[allow(unused)]
    pub cfg: CodegenCfg,
    pub table: TableInfo,
}

impl TemplateTableCS {
    pub fn from(table_def: UniTableDef, cfg: CodegenCfg) -> RS<Self> {
        Ok(Self {
            table: TableInfo::from(table_def, LangKind::CSharp)?,
            cfg,
        })
    }
}
