use crate::contract::field_info::FieldInfo;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone)]
pub struct TableInfo {
    inner: Arc<Mutex<TableInner>>,
}

struct TableInner {
    schema_table: Arc<SchemaTable>,
    name2oid: HashMap<String, OID>,
    oid2column: HashMap<OID, FieldInfo>,
    key_oid: Vec<OID>,
    value_oid: Vec<OID>,
    key_tuple_desc: TupleDesc,
    value_tuple_desc: TupleDesc,
}

impl TableInfo {
    pub fn new(table_schema: SchemaTable) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TableInner::new(table_schema))),
        }
    }

    pub fn table_desc(&self) -> RS<Arc<TableDesc>> {
        let inner = self.inner.lock().unwrap();
        let ret = Arc::new(TableDesc::new(
            inner.name().clone(),
            inner.id(),
            inner.key_oid.clone(),
            inner.value_oid.clone(),
            inner.key_tuple_desc.clone(),
            inner.value_tuple_desc.clone(),
            inner.name2oid.clone(),
            inner.oid2column.clone(),
        ));
        Ok(ret)
    }

    pub fn schema(&self) -> Arc<SchemaTable> {
        self.inner.lock().unwrap().schema_table.clone()
    }
}

impl TableInner {
    pub fn new(table_schema: SchemaTable) -> Self {
        let (key_tuple_desc, key_tuple_payload_info) = table_schema.key_tuple_desc();
        let (value_tuple_desc, value_tuple_payload_info) = table_schema.value_tuple_desc();
        if value_tuple_desc.field_count() != value_tuple_payload_info.len() {
            panic!("field describe length mismatch");
        }
        let mut name2oid = HashMap::new();
        let mut oid2column = HashMap::new();
        let mut key_oid = Vec::new();
        let mut value_oid = Vec::new();
        for (payload, oids) in [
            (key_tuple_payload_info, &mut key_oid),
            (value_tuple_payload_info, &mut value_oid),
        ] {
            for field_info in payload {
                oids.push(field_info.id());
                name2oid.insert(field_info.name().clone(), field_info.id());
                oid2column.insert(field_info.id(), field_info.clone());
            }
        }

        Self {
            schema_table: Arc::new(table_schema),
            name2oid,
            oid2column,
            key_oid,
            value_oid,
            key_tuple_desc,
            value_tuple_desc,
        }
    }

    pub fn id(&self) -> OID {
        self.schema_table.id()
    }

    pub fn name(&self) -> &String {
        self.schema_table.table_name()
    }
}
