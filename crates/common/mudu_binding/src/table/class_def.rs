use crate::table::column_def::ColumnDef;
use crate::universal::uni_data_type::UniDataType;
use crate::universal::uni_struct_type::{UniStructField, UniStructType};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::class_kind::ClassKind;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use std::collections::HashMap;

/// Definition of a table composed of named columns.
#[derive(Debug, Clone)]
pub struct ClassDef {
    class_name: String,
    fields: Vec<ColumnDef>,
    name2fields: HashMap<String, ColumnDef>,
    kind: ClassKind,
}

impl ClassDef {
    /// Updates each column's type in-place to match the given universal record type.
    pub fn update_field_inline(&mut self, ty: &UniDataType) -> RS<()> {
        let record = if let UniDataType::Struct(record) = ty {
            record
        } else {
            return Err(mudu_error!(
                ErrorCode::FatalInternal,
                "expected a record type"
            ));
        };
        if record.struct_name != self.class_name {
            return Err(mudu_error!(ErrorCode::FatalInternal, "expected name equal"));
        }
        if record.struct_fields.len() != self.fields.len() {
            return Err(mudu_error!(
                ErrorCode::FatalInternal,
                "expected table columns equal"
            ));
        }
        for (i, column) in self.fields.iter_mut().enumerate() {
            if column.column_name() != &record.struct_fields[i].field_name {
                return Err(mudu_error!(
                    ErrorCode::FatalInternal,
                    "expected column name equal"
                ));
            }
            column.set_column_type(record.struct_fields[i].field_type.clone());
        }
        Ok(())
    }
    /// Converts this table definition into a universal record type.
    pub fn to_struct_type(&self) -> RS<UniStructType> {
        let mut struct_fields = Vec::with_capacity(self.fields.len());
        for column in self.fields.iter() {
            let field_type = column.data_type().clone();
            let field_name = column.column_name().clone();
            let struct_field = UniStructField {
                field_name,
                field_type,
                field_attrs: Vec::new(),
            };
            struct_fields.push(struct_field)
        }
        Ok(UniStructType {
            struct_name: self.class_name.clone(),
            struct_fields,
        })
    }

    /// Creates a new table definition from a name and column list.
    pub fn new(class_name: String, table_columns: Vec<ColumnDef>) -> Self {
        let mut name2column_def = HashMap::new();
        for c in table_columns.iter() {
            name2column_def.insert(c.column_name().clone(), c.clone());
        }
        Self {
            class_name,
            fields: table_columns,
            name2fields: name2column_def,
            kind: ClassKind::Table,
        }
    }

    /// Returns the table name.
    pub fn class_name(&self) -> &String {
        &self.class_name
    }

    /// Returns the column definitions.
    pub fn table_columns(&self) -> &Vec<ColumnDef> {
        &self.fields
    }

    /// Returns the object kind (table/queue/cache/type).
    pub fn kind(&self) -> ClassKind {
        self.kind
    }

    /// Sets the object kind.
    pub fn set_kind(&mut self, kind: ClassKind) {
        self.kind = kind;
    }

    /// Builds a `TupleFieldDesc` from the table columns.
    pub fn row_desc(&self) -> RS<TupleFieldDesc> {
        let mut vec = vec![];
        for c in &self.fields {
            let dd = DatumDesc::new(
                c.column_name().clone(),
                c.data_type()
                    .clone()
                    .uni_to_with_params(c.data_type_param().clone())?,
            );
            vec.push(dd);
        }
        Ok(TupleFieldDesc::new(vec))
    }

    /// Looks up a column definition by name.
    pub fn find_column_def_by_name(&self, name: &str) -> Option<&ColumnDef> {
        self.name2fields.get(name)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assertions_on_constants
    )]

    use super::*;
    use crate::universal::uni_scalar::UniScalar;
    use crate::universal::uni_struct_type::{UniStructField, UniStructType};
    use mudu::error::ErrorCode;
    use mudu_type::type_family::TypeFamily;

    fn column(name: &str, ty: UniDataType) -> ColumnDef {
        ColumnDef::new(name.to_string(), ty, None, false, false)
    }

    fn sample_class_def() -> ClassDef {
        ClassDef::new(
            "user".to_string(),
            vec![
                column("id", UniDataType::Scalar(UniScalar::I32)),
                column("age", UniDataType::Scalar(UniScalar::I64)),
                column("name", UniDataType::Scalar(UniScalar::String)),
            ],
        )
    }

    #[test]
    fn new_populates_name_and_columns() {
        let table = sample_class_def();
        assert_eq!(table.class_name(), "user");
        assert_eq!(table.table_columns().len(), 3);
        assert_eq!(table.table_columns()[0].column_name(), "id");
        assert_eq!(table.table_columns()[1].column_name(), "age");
        assert_eq!(table.table_columns()[2].column_name(), "name");

        assert!(table.find_column_def_by_name("id").is_some());
        assert!(table.find_column_def_by_name("missing").is_none());
    }

    #[test]
    fn find_column_def_by_name_missing_returns_none() {
        let table = sample_class_def();
        assert!(table.find_column_def_by_name("not_a_column").is_none());
    }

    #[test]
    fn to_struct_type_matches_input() {
        let table = sample_class_def();
        let ty = table.to_struct_type().unwrap();
        assert_eq!(ty.struct_name, "user");
        assert_eq!(ty.struct_fields.len(), 3);
        assert_eq!(ty.struct_fields[0].field_name, "id");
        assert!(matches!(
            ty.struct_fields[0].field_type,
            UniDataType::Scalar(UniScalar::I32)
        ));
        assert_eq!(ty.struct_fields[1].field_name, "age");
        assert!(matches!(
            ty.struct_fields[1].field_type,
            UniDataType::Scalar(UniScalar::I64)
        ));
        assert_eq!(ty.struct_fields[2].field_name, "name");
        assert!(matches!(
            ty.struct_fields[2].field_type,
            UniDataType::Scalar(UniScalar::String)
        ));
    }

    #[test]
    fn update_field_inline_changes_types() {
        let mut table = ClassDef::new(
            "user".to_string(),
            vec![column("id", UniDataType::Scalar(UniScalar::I32))],
        );
        let new_type = UniDataType::Struct(UniStructType {
            struct_name: "user".to_string(),
            struct_fields: vec![UniStructField {
                field_name: "id".to_string(),
                field_type: UniDataType::Scalar(UniScalar::I64),
                field_attrs: Vec::new(),
            }],
        });
        table.update_field_inline(&new_type).unwrap();
        assert!(matches!(
            table.table_columns()[0].data_type(),
            UniDataType::Scalar(UniScalar::I64)
        ));
    }

    #[test]
    fn update_field_inline_rejects_non_record() {
        let mut table = sample_class_def();
        let err = table
            .update_field_inline(&UniDataType::Scalar(UniScalar::I32))
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::FatalInternal);
    }

    #[test]
    fn update_field_inline_rejects_name_mismatch() {
        let mut table = sample_class_def();
        let new_type = UniDataType::Struct(UniStructType {
            struct_name: "other".to_string(),
            struct_fields: table
                .to_struct_type()
                .unwrap()
                .struct_fields
                .into_iter()
                .map(|f| UniStructField {
                    field_name: f.field_name,
                    field_type: f.field_type,
                    field_attrs: Vec::new(),
                })
                .collect(),
        });
        let err = table.update_field_inline(&new_type).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::FatalInternal);
    }

    #[test]
    fn update_field_inline_rejects_field_count_mismatch() {
        let mut table = sample_class_def();
        let new_type = UniDataType::Struct(UniStructType {
            struct_name: "user".to_string(),
            struct_fields: vec![UniStructField {
                field_name: "id".to_string(),
                field_type: UniDataType::Scalar(UniScalar::I32),
                field_attrs: Vec::new(),
            }],
        });
        let err = table.update_field_inline(&new_type).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::FatalInternal);
    }

    #[test]
    fn update_field_inline_rejects_column_name_mismatch() {
        let mut table = sample_class_def();
        let new_type = UniDataType::Struct(UniStructType {
            struct_name: "user".to_string(),
            struct_fields: vec![
                UniStructField {
                    field_name: "id".to_string(),
                    field_type: UniDataType::Scalar(UniScalar::I32),
                    field_attrs: Vec::new(),
                },
                UniStructField {
                    field_name: "age".to_string(),
                    field_type: UniDataType::Scalar(UniScalar::I64),
                    field_attrs: Vec::new(),
                },
                UniStructField {
                    field_name: "renamed".to_string(),
                    field_type: UniDataType::Scalar(UniScalar::String),
                    field_attrs: Vec::new(),
                },
            ],
        });
        let err = table.update_field_inline(&new_type).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::FatalInternal);
    }

    #[test]
    fn row_desc_builds_tuple_field_desc() {
        let table = sample_class_def();
        let desc = table.row_desc().unwrap();
        let fields = desc.fields();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].name(), "id");
        assert_eq!(fields[0].data_type().type_family(), TypeFamily::I32);
        assert_eq!(fields[1].name(), "age");
        assert_eq!(fields[1].data_type().type_family(), TypeFamily::I64);
        assert_eq!(fields[2].name(), "name");
        assert_eq!(fields[2].data_type().type_family(), TypeFamily::String);
    }

    #[test]
    fn row_desc_propagates_unsupported_scalar() {
        let table = ClassDef::new(
            "bad".to_string(),
            vec![column("flag", UniDataType::Scalar(UniScalar::Bool))],
        );
        let err = table.row_desc().unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    }
}
