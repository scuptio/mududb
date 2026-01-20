use mudu::common::id::OID;
use mudu_type::dt_fn_param::DatType;

#[derive(Clone, Debug, Default)]
pub struct FieldInfo {
    name: String,
    id: OID,
    type_desc: DatType,
    // index in key or value tuple
    datum_index: usize,
    // index in original create table column definition list
    column_index: usize,
    is_primary: bool,
}

impl FieldInfo {
    pub fn new(name: String, id: OID, type_desc: DatType, index: usize, is_primary: bool) -> Self {
        Self {
            name,
            id,
            type_desc,
            datum_index: index,
            column_index: index,
            is_primary,
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn id(&self) -> OID {
        self.id
    }

    pub fn column_index(&self) -> usize {
        self.column_index
    }

    pub fn is_primary(&self) -> bool {
        self.is_primary
    }

    pub fn datum_index(&self) -> usize {
        self.datum_index
    }

    pub fn set_datum_index(&mut self, index: usize) {
        self.datum_index = index;
    }

    pub fn type_desc(&self) -> &DatType {
        &self.type_desc
    }
}
