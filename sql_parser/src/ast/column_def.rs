use mudu::data_type::dat_type::DatType;
use mudu::data_type::dt_impl::dat_type_id::DatTypeID;
use mudu::data_type::param_obj::ParamObj;

#[derive(Clone, Debug)]
pub struct ColumnDef {
    column_name: String,
    data_type_def: DatType,
    is_primary_key: bool,
    index: u32,
}

impl ColumnDef {
    pub fn new(column_name: String, data_type_def: DatType, is_primary_key: bool) -> Self {
        Self {
            column_name,
            data_type_def,
            is_primary_key,
            index: u32::MAX,
        }
    }

    pub fn data_type(&self) -> DatTypeID {
        self.data_type_def.id()
    }

    pub fn type_param(&self) -> ParamObj {
        self.data_type_def.param().clone()
    }

    pub fn is_primary_key(&self) -> bool {
        self.is_primary_key
    }

    pub fn type_declare(&self) -> &DatType {
        &self.data_type_def
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
