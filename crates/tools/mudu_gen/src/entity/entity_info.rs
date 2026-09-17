//! Entity metadata extracted from a table definition.

use crate::entity::field_info::FieldInfo;
use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::table::table_def::TableDef;

/// Metadata for a generated entity struct.
#[derive(Debug)]
pub struct EntityInfo {
    /// Raw table/entity name.
    pub entity_name: String,
    /// Struct name in PascalCase.
    pub struct_obj_name: String,
    /// Name of the partial-update changeset struct (`<Struct>Change`).
    pub change_struct_name: String,
    /// Fields of the entity.
    pub fields: Vec<FieldInfo>,
    /// `true` if the table declares a primary key.
    pub has_primary_key: bool,
    /// `true` if a partial-update struct is generated: the table has a
    /// primary key and at least one non-key column.
    pub has_update: bool,
    /// Comma-separated column names in declaration order.
    pub column_name_list: String,
    /// Comma-separated `?` placeholders, one per column.
    pub placeholder_list: String,
    /// Primary-key predicate (`"a = ?"` or `"a = ? AND b = ?"`);
    /// empty when the table has no primary key.
    pub pk_predicate: String,
}

impl EntityInfo {
    /// Build [`EntityInfo`] from a [`TableDef`] and target language.
    pub fn from_record_def(record_def: &TableDef, lang_kind: &LangKind) -> RS<Self> {
        let mut fields = Vec::with_capacity(record_def.table_columns().len());
        for field in record_def.table_columns() {
            let column_info =
                FieldInfo::from_column_schema(record_def.table_name(), field, lang_kind)?;
            fields.push(column_info);
        }
        let table_name = record_def.table_name();
        let struct_obj_name = to_pascal_case(table_name);
        let has_primary_key = fields.iter().any(|f| f.is_primary_key);
        let has_update = has_primary_key && fields.iter().any(|f| !f.is_primary_key);
        let column_name_list = fields
            .iter()
            .map(|f| f.field_name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let placeholder_list = fields.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let pk_predicate = fields
            .iter()
            .filter(|f| f.is_primary_key)
            .map(|f| format!("{} = ?", f.field_name))
            .collect::<Vec<_>>()
            .join(" AND ");
        Ok(Self {
            entity_name: table_name.clone(),
            change_struct_name: format!("{struct_obj_name}Change"),
            struct_obj_name,
            fields,
            has_primary_key,
            has_update,
            column_name_list,
            placeholder_list,
            pk_predicate,
        })
    }

    /// Returns the primary key fields in column declaration order.
    pub fn pk_fields(&self) -> Vec<&FieldInfo> {
        self.fields.iter().filter(|f| f.is_primary_key).collect()
    }

    /// Returns the non-primary-key fields in column declaration order.
    pub fn update_fields(&self) -> Vec<&FieldInfo> {
        self.fields.iter().filter(|f| !f.is_primary_key).collect()
    }
}
