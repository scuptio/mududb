use crate::record::field_def::FieldDef;
use crate::universal::uni_data_type::UniDataType;
use crate::universal::uni_record_type::{UniRecordField, UniRecordType};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use std::collections::HashMap;

/// Definition of a record (table) composed of named fields.
#[derive(Debug, Clone)]
pub struct RecordDef {
    record_name: String,
    fields: Vec<FieldDef>,
    name2fields: HashMap<String, FieldDef>,
}

impl RecordDef {
    /// Updates each field's type in-place to match the given universal record type.
    pub fn update_field_inline(&mut self, ty: &UniDataType) -> RS<()> {
        let record = if let UniDataType::Record(record) = ty {
            record
        } else {
            return Err(mudu_error!(
                ErrorCode::FatalInternal,
                "expected a record type"
            ));
        };
        if record.record_name != self.record_name {
            return Err(mudu_error!(ErrorCode::FatalInternal, "expected name equal"));
        }
        if record.record_fields.len() != self.fields.len() {
            return Err(mudu_error!(
                ErrorCode::FatalInternal,
                "expected table columns equal"
            ));
        }
        for (i, column) in self.fields.iter_mut().enumerate() {
            if column.column_name() != &record.record_fields[i].field_name {
                return Err(mudu_error!(
                    ErrorCode::FatalInternal,
                    "expected column name equal"
                ));
            }
            column.set_column_type(record.record_fields[i].field_type.clone());
        }
        Ok(())
    }
    /// Converts this record definition into a universal record type.
    pub fn to_record_type(&self) -> RS<UniRecordType> {
        let mut record_fields = Vec::with_capacity(self.fields.len());
        for column in self.fields.iter() {
            let field_type = column.data_type().clone();
            let field_name = column.column_name().clone();
            let record_field = UniRecordField {
                field_name,
                field_type,
                field_attrs: Vec::new(),
            };
            record_fields.push(record_field)
        }
        Ok(UniRecordType {
            record_name: self.record_name.clone(),
            record_fields,
        })
    }

    /// Creates a new record definition from a name and field list.
    pub fn new(table_name: String, table_columns: Vec<FieldDef>) -> Self {
        let mut name2column_def = HashMap::new();
        for c in table_columns.iter() {
            name2column_def.insert(c.column_name().clone(), c.clone());
        }
        Self {
            record_name: table_name,
            fields: table_columns,
            name2fields: name2column_def,
        }
    }

    /// Returns the record (table) name.
    pub fn table_name(&self) -> &String {
        &self.record_name
    }

    /// Returns the field (column) definitions.
    pub fn table_columns(&self) -> &Vec<FieldDef> {
        &self.fields
    }

    /// Builds a `TupleFieldDesc` from the record fields.
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

    /// Looks up a field definition by name.
    pub fn find_column_def_by_name(&self, name: &str) -> Option<&FieldDef> {
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
    use crate::universal::uni_record_type::{UniRecordField, UniRecordType};
    use crate::universal::uni_scalar::UniScalar;
    use mudu::error::ErrorCode;
    use mudu_type::type_family::TypeFamily;

    fn field(name: &str, ty: UniDataType) -> FieldDef {
        FieldDef::new(name.to_string(), ty, None, false)
    }

    fn sample_record_def() -> RecordDef {
        RecordDef::new(
            "user".to_string(),
            vec![
                field("id", UniDataType::Scalar(UniScalar::I32)),
                field("age", UniDataType::Scalar(UniScalar::I64)),
                field("name", UniDataType::Scalar(UniScalar::String)),
            ],
        )
    }

    #[test]
    fn new_populates_name_and_columns() {
        let record = sample_record_def();
        assert_eq!(record.table_name(), "user");
        assert_eq!(record.table_columns().len(), 3);
        assert_eq!(record.table_columns()[0].column_name(), "id");
        assert_eq!(record.table_columns()[1].column_name(), "age");
        assert_eq!(record.table_columns()[2].column_name(), "name");

        assert!(record.find_column_def_by_name("id").is_some());
        assert!(record.find_column_def_by_name("missing").is_none());
    }

    #[test]
    fn find_column_def_by_name_missing_returns_none() {
        let record = sample_record_def();
        assert!(record.find_column_def_by_name("not_a_column").is_none());
    }

    #[test]
    fn to_record_type_matches_input() {
        let record = sample_record_def();
        let ty = record.to_record_type().unwrap();
        assert_eq!(ty.record_name, "user");
        assert_eq!(ty.record_fields.len(), 3);
        assert_eq!(ty.record_fields[0].field_name, "id");
        assert!(matches!(
            ty.record_fields[0].field_type,
            UniDataType::Scalar(UniScalar::I32)
        ));
        assert_eq!(ty.record_fields[1].field_name, "age");
        assert!(matches!(
            ty.record_fields[1].field_type,
            UniDataType::Scalar(UniScalar::I64)
        ));
        assert_eq!(ty.record_fields[2].field_name, "name");
        assert!(matches!(
            ty.record_fields[2].field_type,
            UniDataType::Scalar(UniScalar::String)
        ));
    }

    #[test]
    fn update_field_inline_changes_types() {
        let mut record = RecordDef::new(
            "user".to_string(),
            vec![field("id", UniDataType::Scalar(UniScalar::I32))],
        );
        let new_type = UniDataType::Record(UniRecordType {
            record_name: "user".to_string(),
            record_fields: vec![UniRecordField {
                field_name: "id".to_string(),
                field_type: UniDataType::Scalar(UniScalar::I64),
                field_attrs: Vec::new(),
            }],
        });
        record.update_field_inline(&new_type).unwrap();
        assert!(matches!(
            record.table_columns()[0].data_type(),
            UniDataType::Scalar(UniScalar::I64)
        ));
    }

    #[test]
    fn update_field_inline_rejects_non_record() {
        let mut record = sample_record_def();
        let err = record
            .update_field_inline(&UniDataType::Scalar(UniScalar::I32))
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::FatalInternal);
    }

    #[test]
    fn update_field_inline_rejects_name_mismatch() {
        let mut record = sample_record_def();
        let new_type = UniDataType::Record(UniRecordType {
            record_name: "other".to_string(),
            record_fields: record
                .to_record_type()
                .unwrap()
                .record_fields
                .into_iter()
                .map(|f| UniRecordField {
                    field_name: f.field_name,
                    field_type: f.field_type,
                    field_attrs: Vec::new(),
                })
                .collect(),
        });
        let err = record.update_field_inline(&new_type).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::FatalInternal);
    }

    #[test]
    fn update_field_inline_rejects_field_count_mismatch() {
        let mut record = sample_record_def();
        let new_type = UniDataType::Record(UniRecordType {
            record_name: "user".to_string(),
            record_fields: vec![UniRecordField {
                field_name: "id".to_string(),
                field_type: UniDataType::Scalar(UniScalar::I32),
                field_attrs: Vec::new(),
            }],
        });
        let err = record.update_field_inline(&new_type).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::FatalInternal);
    }

    #[test]
    fn update_field_inline_rejects_column_name_mismatch() {
        let mut record = sample_record_def();
        let new_type = UniDataType::Record(UniRecordType {
            record_name: "user".to_string(),
            record_fields: vec![
                UniRecordField {
                    field_name: "id".to_string(),
                    field_type: UniDataType::Scalar(UniScalar::I32),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "age".to_string(),
                    field_type: UniDataType::Scalar(UniScalar::I64),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "renamed".to_string(),
                    field_type: UniDataType::Scalar(UniScalar::String),
                    field_attrs: Vec::new(),
                },
            ],
        });
        let err = record.update_field_inline(&new_type).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::FatalInternal);
    }

    #[test]
    fn row_desc_builds_tuple_field_desc() {
        let record = sample_record_def();
        let desc = record.row_desc().unwrap();
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
        let record = RecordDef::new(
            "bad".to_string(),
            vec![field("flag", UniDataType::Scalar(UniScalar::Bool))],
        );
        let err = record.row_desc().unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    }
}
