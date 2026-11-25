use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::struct_fields::to_field_info;
use crate::lang_impl::lang::struct_info::StructFieldInfo;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_def::UniClassDef;
use std::collections::HashMap;

/// Language-normalized table metadata.
#[derive(Debug, Clone)]
pub struct TableInfo {
    /// Table doc comments.
    pub table_comments: String,
    /// Pascal-case table name.
    pub class_name: String,
    /// Key fields.
    pub key_fields: Vec<StructFieldInfo>,
    /// Value fields.
    pub value_fields: Vec<StructFieldInfo>,
}

impl TableInfo {
    /// Convert a [`UniClassDef`] into a [`TableInfo`] for the target language.
    pub fn from(
        class_def: UniClassDef,
        lang: LangKind,
        type_kinds: &HashMap<String, String>,
    ) -> RS<Self> {
        let class_name = to_pascal_case(&class_def.class_name);
        let key_fields = to_field_info(&class_def.table_key, &lang, type_kinds)?;
        let value_fields = to_field_info(&class_def.table_value, &lang, type_kinds)?;
        let struct_info = Self {
            table_comments: String::new(),
            class_name,
            key_fields,
            value_fields,
        };
        Ok(struct_info)
    }
}

#[cfg(test)]
mod tests {
    use super::TableInfo;
    use crate::lang_impl::lang::lang_kind::LangKind;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_def::{StructField, UniClassDef};
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn from_normalizes_table_metadata() -> RS<()> {
        let class_def = UniClassDef {
            table_comments: "comment".to_string(),
            class_name: "my-table".to_string(),
            table_key: vec![StructField::new(
                "id".to_string(),
                "id".to_string(),
                UniDataType::Scalar(UniScalar::I32),
            )],
            table_value: vec![StructField::new(
                "name".to_string(),
                "name".to_string(),
                UniDataType::Scalar(UniScalar::String),
            )],
        };
        let info = TableInfo::from(class_def, LangKind::CSharp, &Default::default())?;
        assert_eq!(info.class_name, "MyTable");
        assert_eq!(info.key_fields.len(), 1);
        assert_eq!(info.value_fields.len(), 1);
        assert_eq!(info.key_fields[0].rf_type, "int");
        assert_eq!(info.value_fields[0].rf_type, "string");
        Ok(())
    }
}
