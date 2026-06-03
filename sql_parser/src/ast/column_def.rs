use mudu::common::id::AttrIndex;
use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_dat_value::UniDatValue;

#[derive(Clone, Debug)]
pub struct ColumnDef {
    column_name: String,
    data_type_def: UniDatType,
    data_type_param: Option<Vec<UniDatValue>>,
    opt_primary_key_index: Option<AttrIndex>,
    nullable: bool,
    index: AttrIndex,
}

impl ColumnDef {
    pub fn new(
        column_name: String,
        data_type_def: UniDatType,
        data_type_param: Option<Vec<UniDatValue>>,
    ) -> Self {
        Self {
            column_name,
            data_type_def,
            data_type_param,
            opt_primary_key_index: None,
            nullable: true,
            index: AttrIndex::MAX,
        }
    }

    pub fn data_type(&self) -> &UniDatType {
        &self.data_type_def
    }

    pub fn data_type_param(&self) -> &Option<Vec<UniDatValue>> {
        &self.data_type_param
    }

    pub fn is_primary_key(&self) -> bool {
        self.opt_primary_key_index.is_some()
    }

    pub fn column_name(&self) -> &String {
        &self.column_name
    }

    pub fn primary_key_index(&self) -> Option<AttrIndex> {
        self.opt_primary_key_index
    }

    pub fn expect_primary_key_index(&self) -> AttrIndex {
        self.opt_primary_key_index.unwrap()
    }

    pub fn set_primary_key_index(&mut self, index: Option<AttrIndex>) {
        self.opt_primary_key_index = index;
        if index.is_some() {
            self.nullable = false;
        }
    }

    pub fn nullable(&self) -> bool {
        self.nullable
    }

    pub fn set_nullable(&mut self, nullable: bool) {
        self.nullable = nullable;
    }

    pub fn set_index(&mut self, index: AttrIndex) {
        self.index = index;
    }

    // column index in table schema
    pub fn column_index(&self) -> AttrIndex {
        self.index
    }
}
