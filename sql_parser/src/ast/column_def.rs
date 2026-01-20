use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_dat_value::UniDatValue;

#[derive(Clone, Debug)]
pub struct ColumnDef {
    column_name: String,
    data_type_def: UniDatType,
    data_type_param: Option<Vec<UniDatValue>>,
    is_primary_key: bool,
    index: u32,
}

impl ColumnDef {
    pub fn new(column_name: String, data_type_def: UniDatType, data_type_param: Option<Vec<UniDatValue>>, is_primary_key: bool) -> Self {
        Self {
            column_name,
            data_type_def,
            data_type_param,
            is_primary_key,
            index: u32::MAX,
        }
    }

    pub fn data_type(&self) -> &UniDatType {
        &self.data_type_def
    }

    pub fn data_type_param(&self) -> &Option<Vec<UniDatValue>> {
        &self.data_type_param
    }

    pub fn is_primary_key(&self) -> bool {
        self.is_primary_key
    }


    pub fn column_name(&self) -> &String {
        &self.column_name
    }

    pub fn set_primary_key(&mut self, is_primary: bool) {
        self.is_primary_key = is_primary;
    }

    pub fn set_index(&mut self, index: u32) {
        self.index = index;
    }

    // column index in table schema
    pub fn column_index(&self) -> u32 {
        self.index
    }
}
