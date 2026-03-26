use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::record_fields::to_field_info;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_def::UniRecordDef;

#[derive(Debug, Clone)]
pub struct RecordInfo {
    pub record_comments: String,
    pub record_name: String,
    pub record_fields: Vec<RecordFieldInfo>,
}

#[derive(Debug, Clone)]
pub struct RecordFieldInfo {
    pub rf_index: u32,
    pub rf_comments: String,
    pub rf_name: String,
    pub rf_type: String,
    pub rf_required: bool,
    pub rf_default_value: String,
}

impl RecordInfo {
    pub fn from(record_def: UniRecordDef, lang: LangKind) -> RS<Self> {
        let name = to_pascal_case(&record_def.record_name);
        let record_fields = to_field_info(&record_def.record_fields, &lang)?;
        let record_info = RecordInfo {
            record_comments: record_def.record_comments,
            record_name: name,
            record_fields,
        };
        Ok(record_info)
    }
}
