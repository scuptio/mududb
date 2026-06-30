use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::table_info::TableInfo;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::UniTableDef;

/// Askama template for a C# table.
#[derive(Template)]
#[template(path = "csharp/table.cs.jinja", escape = "none")]
pub struct TemplateTableCS {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Normalized table metadata.
    pub table: TableInfo,
}

impl TemplateTableCS {
    /// Build the template from a WIT table definition.
    pub fn from(table_def: UniTableDef, cfg: CodegenCfg) -> RS<Self> {
        Ok(Self {
            table: TableInfo::from(table_def, LangKind::CSharp)?,
            cfg,
        })
    }
}

#[cfg(test)]
#[path = "template_table_cs_test.rs"]
mod template_table_cs_test;
