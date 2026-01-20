use crate::universal::uni_dat_type::UniDatType;

#[derive(Debug, Clone)]
pub struct FieldDef {
    field_name: String,
    data_type: UniDatType,
    not_null: bool,
}

impl FieldDef {
    pub fn new(column_name: String, data_type: UniDatType, not_null: bool) -> Self {
        Self {
            field_name: column_name,
            data_type,
            not_null,
        }
    }

    pub fn column_name(&self) -> &String {
        &self.field_name
    }

    pub fn dat_type(&self) -> &UniDatType {
        &self.data_type
    }

    pub fn is_not_null(&self) -> bool {
        self.not_null
    }

    pub fn set_column_type(&mut self, column_type: UniDatType) {
        self.data_type = column_type;
    }
}
