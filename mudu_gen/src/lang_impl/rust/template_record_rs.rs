use crate::lang_impl::lang::lang_kind::LangKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;

use crate::lang_impl::lang::record_info::RecordInfo;
use mudu_binding::universal::uni_def::UniRecordDef;

#[derive(Template)]
#[template(path = "rust/record.rs.jinja", escape = "none")]
pub struct TemplateRecordRS {
    pub cfg:CodegenCfg,
    pub record: RecordInfo,
}

impl TemplateRecordRS {
    pub fn from(record_def: UniRecordDef, cfg:CodegenCfg) -> RS<Self> {
        Ok(Self {
            record: RecordInfo::from(record_def, LangKind::Rust)?,
            cfg
        })
    }
}