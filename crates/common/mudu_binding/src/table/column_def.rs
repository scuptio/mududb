use crate::universal::uni_data_type::UniDataType;
use crate::universal::uni_data_value::UniDataValue;

/// Definition of a single table column.
#[derive(Debug, Clone)]
pub struct ColumnDef {
    column_name: String,
    data_type: UniDataType,
    data_type_param: Option<Vec<UniDataValue>>,
    is_not_null: bool,
    is_primary_key: bool,
}

impl ColumnDef {
    /// Creates a new column definition.
    pub fn new(
        column_name: String,
        data_type: UniDataType,
        data_type_param: Option<Vec<UniDataValue>>,
        is_not_null: bool,
        is_primary_key: bool,
    ) -> Self {
        Self {
            column_name,
            data_type,
            data_type_param,
            is_not_null,
            is_primary_key,
        }
    }

    /// Returns the column name.
    pub fn column_name(&self) -> &String {
        &self.column_name
    }

    /// Returns the data type.
    pub fn data_type(&self) -> &UniDataType {
        &self.data_type
    }

    /// Returns the optional data type parameters.
    pub fn data_type_param(&self) -> &Option<Vec<UniDataValue>> {
        &self.data_type_param
    }

    /// Returns `true` if the column is declared `NOT NULL`
    /// (primary key columns imply `NOT NULL`).
    pub fn is_not_null(&self) -> bool {
        self.is_not_null
    }

    /// Returns `true` if the column is part of the primary key.
    pub fn is_primary_key(&self) -> bool {
        self.is_primary_key
    }

    /// Sets the column type.
    pub fn set_column_type(&mut self, column_type: UniDataType) {
        self.data_type = column_type;
    }
}
