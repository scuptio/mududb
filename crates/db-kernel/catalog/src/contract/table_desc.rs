use mudu::common::id::{AttrIndex, OID};

use crate::contract::field_info::FieldInfo;
use mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use std::collections::HashMap;

/// Immutable runtime table descriptor: tuple descriptors and field lookups
/// derived from the table schema.
pub struct TableDesc {
    name: String,
    schema: String,
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

/// Parameters for building a [`TableDesc`].
pub struct TableDescParams {
    /// Bare table name.
    pub name: String,
    /// Schema (namespace) the table belongs to.
    pub schema: String,
    /// Table oid.
    pub oid: OID,
    /// Oids of the key fields in key order.
    pub key_oid: Vec<OID>,
    /// Oids of the value fields in value order.
    pub value_oid: Vec<OID>,
    /// Original column indices of the key fields.
    pub key_indices: Vec<AttrIndex>,
    /// Original column indices of the value fields.
    pub value_indices: Vec<AttrIndex>,
    /// All fields in original column order.
    pub fields: Vec<FieldInfo>,
    /// Binary tuple descriptor of the key tuple.
    pub key_desc: TupleDesc,
    /// Binary tuple descriptor of the value tuple.
    pub value_desc: TupleDesc,
    /// Field name to field oid map.
    pub name2oid: HashMap<String, OID>,
    /// Field oid to field info map.
    pub oid2col: HashMap<OID, FieldInfo>,
}

impl TableDesc {
    /// Builds the descriptor from `params`.
    pub fn new(params: TableDescParams) -> Self {
        let column_oid = params.fields.iter().map(|field| field.id()).collect();
        Self {
            name: params.name,
            schema: params.schema,
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

    /// Oids of the key fields in key order.
    pub fn key_field_oid(&self) -> &Vec<OID> {
        &self.key_oid
    }

    /// Oids of the value fields in value order.
    pub fn value_field_oid(&self) -> &Vec<OID> {
        &self.value_oid
    }

    /// The field at `index`. [`AttrIndex`] always refers to the original
    /// table column order; use [`FieldInfo::datum_index`] to locate the field
    /// inside the key/value tuple.
    pub fn get_attr(&self, index: AttrIndex) -> &FieldInfo {
        &self.fields[index]
    }

    /// All fields in original column order.
    pub fn fields(&self) -> &Vec<FieldInfo> {
        &self.fields
    }

    /// Original column indices of the key fields.
    pub fn key_indices(&self) -> &Vec<AttrIndex> {
        &self.key_indices
    }

    /// Original column indices of the value fields.
    pub fn value_indices(&self) -> &Vec<AttrIndex> {
        &self.value_indices
    }

    /// The key fields in key order.
    pub fn key_info(&self) -> Vec<&FieldInfo> {
        self.key_indices
            .iter()
            .map(|index| &self.fields[*index])
            .collect()
    }

    /// Binary descriptor of the key tuple.
    pub fn key_desc(&self) -> &TupleDesc {
        &self.key_desc
    }

    /// The value fields in value order.
    pub fn value_info(&self) -> Vec<&FieldInfo> {
        self.value_indices
            .iter()
            .map(|index| &self.fields[*index])
            .collect()
    }

    /// Binary descriptor of the value tuple.
    pub fn value_desc(&self) -> &TupleDesc {
        &self.value_desc
    }

    /// Field name to oid map.
    pub fn name2oid(&self) -> &HashMap<String, OID> {
        &self.name2oid
    }

    /// Field oid to field info map.
    pub fn oid2col(&self) -> &HashMap<OID, FieldInfo> {
        &self.oid2col
    }

    /// The bare table name.
    pub fn name(&self) -> &String {
        &self.name
    }

    /// Return the schema (namespace) this table belongs to.
    pub fn schema(&self) -> &String {
        &self.schema
    }

    /// The table oid.
    pub fn id(&self) -> OID {
        self.oid
    }

    /// Field oids in original column order.
    pub fn original_column_oid(&self) -> &Vec<OID> {
        &self.column_oid
    }
}
