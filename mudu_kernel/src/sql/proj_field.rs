use mudu::common::id::OID;
use mudu_type::dat_type::DatType;

#[derive(Debug, Clone)]
pub struct ProjField {
    oid: OID,
    index: usize,
    name: String,
    type_desc: DatType,
}

impl ProjField {
    pub fn new(index: usize, oid: OID, name: String, type_desc: DatType) -> Self {
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

    pub fn type_desc(&self) -> &DatType {
        &self.type_desc
    }
}
