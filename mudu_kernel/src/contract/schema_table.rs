use crate::contract::field_info::FieldInfo;
use crate::contract::schema_column::SchemaColumn;
#[cfg(any(test, feature = "test"))]
use arbitrary::{Arbitrary, Unstructured};
use mudu::common::id::{gen_oid, OID};
use mudu::data_type::type_desc::TypeDesc;
use mudu::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use serde::{Deserialize, Serialize};
#[cfg(any(test, feature = "test"))]
use test_utils::_arb_limit;


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SchemaTable {
    oid: OID,
    table_name: String,
    key_columns: Vec<SchemaColumn>,
    value_columns: Vec<SchemaColumn>,
}

pub fn schema_columns_to_tuple_desc(fields: &[SchemaColumn]) -> (TupleDesc, Vec<FieldInfo>) {
    let desc: Vec<(TypeDesc, FieldInfo)> = fields
        .iter()
        .enumerate()
        .map(|(index, c)| {
            let td = TypeDesc::new(c.type_id(), c.type_param().clone());
            let field_info = FieldInfo::new(
                c.get_name().clone(),
                c.get_oid(),
                td.clone(),
                index,
                c.is_primary(),
            );
            (td, field_info)
        })
        .collect();

    assert_eq!(desc.len(), fields.len());
    let (vec_tuple_desc, mut vec_payload) = TupleDesc::normalized_type_desc_vec(desc);
    for (i, f) in vec_payload.iter_mut().enumerate() {
        f.set_datum_index(i);
    }
    let tuple_desc = TupleDesc::from(vec_tuple_desc);
    (tuple_desc, vec_payload)
}

#[cfg(any(test, feature = "test"))]
impl<'a> Arbitrary<'a> for SchemaTable {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let name = String::arbitrary(u)?;
        let v1 = u32::arbitrary(u)?;
        let v2 = u32::arbitrary(u)?;
        let n1 = v1 % _arb_limit::_ARB_MAX_TUPLE_KEY_FIELD as u32 + 1;
        let n2 = v2 % _arb_limit::_ARB_MAX_TUPLE_VALUE_FIELD as u32 + 1;
        let mut primary_key_fields = vec![];
        let mut value_fields = vec![];
        for _i in 0..n1 {
            let s = SchemaColumn::arbitrary(u)?;
            primary_key_fields.push(s);
        }
        for _i in 0..n2 {
            let s = SchemaColumn::arbitrary(u)?;
            value_fields.push(s);
        }
        let schema = SchemaTable::new(name, primary_key_fields, value_fields);
        Ok(schema)
    }
}

impl SchemaTable {
    pub fn new(
        table_name: String,
        key_columns: Vec<SchemaColumn>,
        value_columns: Vec<SchemaColumn>,
    ) -> Self {
        let mut s = SchemaTable {
            oid: gen_oid(),
            table_name,
            key_columns,
            value_columns,
        };
        for (i, sc) in s.key_columns.iter_mut().enumerate() {
            sc.set_primary(true);
            sc.set_index(i as u32);
        }
        for (i, sc) in s.value_columns.iter_mut().enumerate() {
            sc.set_primary(false);
            sc.set_index(i as u32);
        }
        s
    }

    pub fn id(&self) -> OID {
        self.oid
    }

    pub fn table_name(&self) -> &String {
        &self.table_name
    }

    pub fn key_columns(&self) -> &Vec<SchemaColumn> {
        &self.key_columns
    }

    pub fn value_columns(&self) -> &Vec<SchemaColumn> {
        &self.value_columns
    }

    pub fn key_tuple_desc(&self) -> (TupleDesc, Vec<FieldInfo>) {
        schema_columns_to_tuple_desc(&self.key_columns)
    }

    pub fn value_tuple_desc(&self) -> (TupleDesc, Vec<FieldInfo>) {
        schema_columns_to_tuple_desc(&self.value_columns)
    }
}
