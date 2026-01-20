use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::universal::uni_def::UniRecordDef;

#[derive(Debug, Clone)]
pub struct RecordInfo {
    pub record_comments: String,
    pub record_name: String,
    pub record_fields: Vec<RecordFieldInfo>,
}

#[derive(Debug, Clone)]
pub struct RecordFieldInfo {
    pub rf_index:u32,
    pub rf_comments: String,
    pub rf_name: String,
    pub rf_type: String,
}

impl RecordInfo {
    pub fn from(record_def: UniRecordDef, lang: LangKind) -> RS<Self> {
        let name = to_pascal_case(&record_def.record_name);
        let mut fields_ru = Vec::new();
        for (i, field) in record_def.record_fields.iter().enumerate() {
            let field_name = if lang == LangKind::CSharp {
                to_pascal_case(&field.rf_name)
            } else {
                to_snake_case(&field.rf_name)
            };
            let field_type = uni_data_type_to_name(&field.rf_type, &lang)?;
            let field_ru = RecordFieldInfo {
                rf_index: i as _,
                rf_comments: field.rf_comments.clone(),
                rf_name: field_name,
                rf_type: field_type };
            fields_ru.push(field_ru);
        }

        let record_info = RecordInfo {
            record_comments: record_def.record_comments,
            record_name: name,
            record_fields: fields_ru,
        };
        Ok(record_info)
    }
}