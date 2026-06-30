use crate::universal::uni_dat_type::UniDatType;
use crate::universal::uni_dat_value::UniDatValue;

/// Definition of a single record field (column).
#[derive(Debug, Clone)]
pub struct FieldDef {
    field_name: String,
    data_type: UniDatType,
    data_type_param: Option<Vec<UniDatValue>>,
    not_null: bool,
}

impl FieldDef {
    /// Creates a new field definition.
    pub fn new(
        column_name: String,
        data_type: UniDatType,
        data_type_param: Option<Vec<UniDatValue>>,
        not_null: bool,
    ) -> Self {
        Self {
            field_name: column_name,
            data_type,
            data_type_param,
            not_null,
        }
    }

    /// Returns the column name.
    pub fn column_name(&self) -> &String {
        &self.field_name
    }

    /// Returns the data type.
    pub fn dat_type(&self) -> &UniDatType {
        &self.data_type
    }

    /// Returns the optional data type parameters.
    pub fn data_type_param(&self) -> &Option<Vec<UniDatValue>> {
        &self.data_type_param
    }

    /// Returns `true` if the column is declared `NOT NULL`.
    pub fn is_not_null(&self) -> bool {
        self.not_null
    }

    /// Sets the column type.
    pub fn set_column_type(&mut self, column_type: UniDatType) {
        self.data_type = column_type;
    }
}
