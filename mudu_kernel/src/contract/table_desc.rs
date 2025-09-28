use mudu::common::id::OID;

use crate::contract::field_info::FieldInfo;
use mudu::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use std::collections::HashMap;

pub struct TableDesc {
    name: String,
    oid: OID,
    key_oid: Vec<OID>,
    value_oid: Vec<OID>,
    key_desc: TupleDesc,
    value_desc: TupleDesc,
    name2oid: HashMap<String, OID>,
    oid2col: HashMap<OID, FieldInfo>,
    column_oid: Vec<OID>,
}

impl TableDesc {
    pub fn new(
        name: String,
        oid: OID,
        key_oid: Vec<OID>,
        value_oid: Vec<OID>,
        key_desc: TupleDesc,
        value_desc: TupleDesc,
        name2oid: HashMap<String, OID>,
        oid2col: HashMap<OID, FieldInfo>,
    ) -> Self {
        let mut vec: Vec<(&OID, &FieldInfo)> = oid2col.iter().collect();
        vec.sort_by(|a, b| a.1.column_index().cmp(&b.1.column_index()));
        let column_oid: Vec<OID> = vec.iter().map(|(id, _)| *(*id)).collect();
        Self {
            name,
            oid,
            key_oid,
            value_oid,
            key_desc,
            value_desc,
            oid2col,
            name2oid,
            column_oid,
        }
    }

    pub fn key_field_oid(&self) -> &Vec<OID> {
        &self.key_oid
    }

    pub fn value_field_oid(&self) -> &Vec<OID> {
        &self.value_oid
    }

    pub fn key_desc(&self) -> &TupleDesc {
        &self.key_desc
    }

    pub fn value_desc(&self) -> &TupleDesc {
        &self.value_desc
    }

    pub fn name2oid(&self) -> &HashMap<String, OID> {
        &self.name2oid
    }
    pub fn oid2col(&self) -> &HashMap<OID, FieldInfo> {
        &self.oid2col
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn id(&self) -> OID {
        self.oid
    }

    pub fn original_column_oid(&self) -> &Vec<OID> {
        &self.column_oid
    }
}
