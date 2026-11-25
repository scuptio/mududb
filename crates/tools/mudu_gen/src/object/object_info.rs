//! Object metadata extracted from a table definition.

use crate::lang_impl::lang::lang_kind::LangKind;
use crate::object::field_info::FieldInfo;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::table::class_def::ClassDef;

/// Metadata for a generated object struct.
#[derive(Debug)]
pub struct ObjectInfo {
    /// Raw table/object name.
    pub class_name: String,
    /// Struct name in PascalCase.
    pub struct_obj_name: String,
    /// Name of the partial-update changeset struct (`<Struct>Change`).
    pub change_struct_name: String,
    /// Fields of the object.
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

impl ObjectInfo {
    /// Build [`ObjectInfo`] from a [`ClassDef`] and target language.
    pub fn from_struct_def(record_def: &ClassDef, lang_kind: &LangKind) -> RS<Self> {
        let mut fields = Vec::with_capacity(record_def.table_columns().len());
        for field in record_def.table_columns() {
            let column_info =
                FieldInfo::from_column_schema(record_def.class_name(), field, lang_kind)?;
            fields.push(column_info);
        }
        let class_name = record_def.class_name();
        let struct_obj_name = to_pascal_case(class_name);
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
            class_name: class_name.clone(),
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
