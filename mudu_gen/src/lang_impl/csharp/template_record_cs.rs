use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::record_info::RecordInfo;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::UniRecordDef;

#[derive(Template)]
#[template(path = "csharp/record.cs.jinja", escape = "none")]
pub struct TemplateRecordCS {
    #[allow(unused)]
    pub cfg:CodegenCfg,
    pub record: RecordInfo,
}

impl TemplateRecordCS {
    pub fn from(record_def: UniRecordDef, cfg:CodegenCfg) -> RS<Self> {
        Ok(Self {
            record: RecordInfo::from(record_def, LangKind::CSharp)?,
            cfg
        })
    }
}