use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::record_fields::to_field_info;
use crate::lang_impl::lang::record_info::RecordFieldInfo;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_def::UniTableDef;

/// Language-normalized table metadata.
#[derive(Debug, Clone)]
pub struct TableInfo {
    /// Table doc comments.
    pub table_comments: String,
    /// Pascal-case table name.
    pub table_name: String,
    /// Key fields.
    pub key_fields: Vec<RecordFieldInfo>,
    /// Value fields.
    pub value_fields: Vec<RecordFieldInfo>,
}

impl TableInfo {
    /// Convert a [`UniTableDef`] into a [`TableInfo`] for the target language.
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

#[cfg(test)]
mod tests {
    use super::TableInfo;
    use crate::lang_impl::lang::lang_kind::LangKind;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_dat_type::UniDatType;
    use mudu_binding::universal::uni_def::{RecordField, UniTableDef};
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn from_normalizes_table_metadata() -> RS<()> {
        let table_def = UniTableDef {
            table_comments: "comment".to_string(),
            table_name: "my-table".to_string(),
            table_key: vec![RecordField {
                rf_comments: "id".to_string(),
                rf_name: "id".to_string(),
                rf_type: UniDatType::Scalar(UniScalar::I32),
            }],
            table_value: vec![RecordField {
                rf_comments: "name".to_string(),
                rf_name: "name".to_string(),
                rf_type: UniDatType::Scalar(UniScalar::String),
            }],
        };
        let info = TableInfo::from(table_def, LangKind::CSharp)?;
        assert_eq!(info.table_name, "MyTable");
        assert_eq!(info.key_fields.len(), 1);
        assert_eq!(info.value_fields.len(), 1);
        assert_eq!(info.key_fields[0].rf_type, "int");
        assert_eq!(info.value_fields[0].rf_type, "string");
        Ok(())
    }
}
