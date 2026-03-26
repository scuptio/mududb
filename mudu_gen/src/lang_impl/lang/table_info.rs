use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::record_fields::to_field_info;
use crate::lang_impl::lang::record_info::RecordFieldInfo;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_def::UniTableDef;

#[derive(Debug, Clone)]
pub struct TableInfo {
    pub table_comments: String,
    pub table_name: String,
    pub key_fields: Vec<RecordFieldInfo>,
    pub value_fields: Vec<RecordFieldInfo>,
}

impl TableInfo {
    pub fn from(table_def: UniTableDef, lang: LangKind) -> RS<Self> {
        let table_name = to_pascal_case(&table_def.table_name);
        let key_fields = to_field_info(&table_def.table_key, &lang)?;
        let value_fields = to_field_info(&table_def.table_value, &lang)?;
        let record_info = Self {
            table_comments: String::new(),
            table_name,
            key_fields,
            value_fields,
        };
        Ok(record_info)
    }
}
