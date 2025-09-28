use mudu::common::id::OID;
use mudu::data_type::type_desc::TypeDesc;

#[derive(Debug, Clone)]
pub struct ProjField {
    oid: OID,
    index: usize,
    name: String,
    type_desc: TypeDesc,
}

impl ProjField {
    pub fn new(index: usize, oid: OID, name: String, type_desc: TypeDesc) -> Self {
        Self {
            oid,
            index,
            name,
            type_desc,
        }
    }

    pub fn index_of_tuple(&self) -> usize {
        self.index
    }
    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn type_desc(&self) -> &TypeDesc {
        &self.type_desc
    }
}
