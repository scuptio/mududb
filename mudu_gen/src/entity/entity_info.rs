//! Entity metadata extracted from a record definition.

use crate::entity::field_info::FieldInfo;
use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case_upper};
use mudu_binding::record::record_def::RecordDef;

/// Metadata for a generated entity struct.
#[derive(Debug)]
pub struct EntityInfo {
    /// Raw table/entity name.
    pub entity_name: String,
    /// Struct name in PascalCase.
    pub struct_obj_name: String,
    /// Upper-snake-case constant name for the entity.
    pub entity_name_const: String,
    /// Fields of the entity.
    pub fields: Vec<FieldInfo>,
}

impl EntityInfo {
    /// Build [`EntityInfo`] from a [`RecordDef`] and target language.
    pub fn from_record_def(record_def: &RecordDef, lang_kind: &LangKind) -> RS<Self> {
        let mut fields = Vec::with_capacity(record_def.table_columns().len());
        for field in record_def.table_columns() {
            let column_info =
                FieldInfo::from_column_schema(record_def.table_name(), field, lang_kind)?;
            fields.push(column_info);
        }
        Ok(Self {
            entity_name: record_def.table_name().clone(),
            struct_obj_name: to_pascal_case(record_def.table_name()),
            entity_name_const: to_snake_case_upper(record_def.table_name()),
            fields,
        })
    }
}
