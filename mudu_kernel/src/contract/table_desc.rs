use mudu::common::id::{AttrIndex, OID};

use crate::contract::field_info::FieldInfo;
use mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use std::collections::HashMap;

pub struct TableDesc {
    name: String,
    oid: OID,
    key_oid: Vec<OID>,
    value_oid: Vec<OID>,

    // AttrIndex is the column order in the original table definition.
    key_desc: TupleDesc,
    value_desc: TupleDesc,
    fields: Vec<FieldInfo>,
    key_indices: Vec<AttrIndex>,
    value_indices: Vec<AttrIndex>,
    name2oid: HashMap<String, OID>,
    oid2col: HashMap<OID, FieldInfo>,
    column_oid: Vec<OID>,
}

pub struct TableDescParams {
    pub name: String,
    pub oid: OID,
    pub key_oid: Vec<OID>,
    pub value_oid: Vec<OID>,
    pub key_indices: Vec<AttrIndex>,
    pub value_indices: Vec<AttrIndex>,
    pub fields: Vec<FieldInfo>,
    pub key_desc: TupleDesc,
    pub value_desc: TupleDesc,
    pub name2oid: HashMap<String, OID>,
    pub oid2col: HashMap<OID, FieldInfo>,
}

impl TableDesc {
    pub fn new(params: TableDescParams) -> Self {
        let column_oid = params.fields.iter().map(|field| field.id()).collect();
        Self {
            name: params.name,
            oid: params.oid,
            key_oid: params.key_oid,
            value_oid: params.value_oid,
            key_desc: params.key_desc,
            value_desc: params.value_desc,
            fields: params.fields,
            key_indices: params.key_indices,
            value_indices: params.value_indices,
            oid2col: params.oid2col,
            name2oid: params.name2oid,
            column_oid,
        }
    }

    pub fn key_field_oid(&self) -> &Vec<OID> {
        &self.key_oid
    }

    pub fn value_field_oid(&self) -> &Vec<OID> {
        &self.value_oid
    }

    // AttrIndex always refers to the original table column order.
    // Use FieldInfo.datum_index() to locate the field inside the key/value tuple.
    pub fn get_attr(&self, index: AttrIndex) -> &FieldInfo {
        &self.fields[index]
    }

    pub fn fields(&self) -> &Vec<FieldInfo> {
        &self.fields
    }

    pub fn key_indices(&self) -> &Vec<AttrIndex> {
        &self.key_indices
    }

    pub fn value_indices(&self) -> &Vec<AttrIndex> {
        &self.value_indices
    }

    pub fn key_info(&self) -> Vec<&FieldInfo> {
        self.key_indices
            .iter()
            .map(|index| &self.fields[*index])
            .collect()
    }
    pub fn key_desc(&self) -> &TupleDesc {
        &self.key_desc
    }

    pub fn value_info(&self) -> Vec<&FieldInfo> {
        self.value_indices
            .iter()
            .map(|index| &self.fields[*index])
            .collect()
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
