use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::record_info::RecordInfo;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::UniRecordDef;

/// Askama template for an AssemblyScript record.
#[derive(Template)]
#[template(path = "assemblyscript/record.ts.jinja", escape = "none")]
pub struct TemplateRecordAS {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Normalized record metadata.
    pub record: RecordInfo,
}

impl TemplateRecordAS {
    /// Build the template from a WIT record definition.
    pub fn from(record_def: UniRecordDef, cfg: CodegenCfg) -> RS<Self> {
        Ok(Self {
            record: RecordInfo::from(record_def, LangKind::AssemblyScript)?,
            cfg,
        })
    }
}
