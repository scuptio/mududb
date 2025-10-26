use mudu::data_type::dat_type::DatType;

#[derive(Debug, Clone)]
pub struct TableColumnDef {
    column_name: String,
    data_type: DatType,
    is_unique: bool,
    not_null: bool,
}

impl TableColumnDef {
    pub fn new(
        column_name: String,
        data_type: DatType,
        is_unique: bool,
        not_null: bool,
    ) -> Self {
        Self {
            column_name,
            data_type,
            is_unique,
            not_null,
        }
    }

    pub fn column_name(&self) -> &String {
        &self.column_name
    }

    pub fn data_type(&self) -> &DatType {
        &self.data_type
    }

    pub fn is_unique(&self) -> bool {
        self.is_unique
    }

    pub fn is_not_null(&self) -> bool {
        self.not_null
    }
}