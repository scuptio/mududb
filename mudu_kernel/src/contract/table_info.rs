use crate::contract::field_info::FieldInfo;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::{TableDesc, TableDescParams};
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use mudu_sys::sync::SMutex;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct TableInfo {
    inner: Arc<SMutex<TableInner>>,
}

struct TableInner {
    schema_table: Arc<SchemaTable>,
    name2oid: HashMap<String, OID>,
    oid2column: HashMap<OID, FieldInfo>,
    fields: Vec<FieldInfo>,
    key_oid: Vec<OID>,
    value_oid: Vec<OID>,
    key_indices: Vec<AttrIndex>,
    value_indices: Vec<AttrIndex>,
    key_tuple_desc: TupleDesc,
    value_tuple_desc: TupleDesc,
}

impl TableInfo {
    pub fn new(table_schema: SchemaTable) -> RS<Self> {
        Ok(Self {
            inner: Arc::new(SMutex::new(TableInner::new(table_schema)?)),
        })
    }

    pub fn table_desc(&self) -> RS<Arc<TableDesc>> {
        let inner = self.inner.lock().unwrap();
        let ret = Arc::new(TableDesc::new(TableDescParams {
            name: inner.name().clone(),
            oid: inner.id(),
            key_oid: inner.key_oid.clone(),
            value_oid: inner.value_oid.clone(),
            key_indices: inner.key_indices.clone(),
            value_indices: inner.value_indices.clone(),
            fields: inner.fields.clone(),
            key_desc: inner.key_tuple_desc.clone(),
            value_desc: inner.value_tuple_desc.clone(),
            name2oid: inner.name2oid.clone(),
            oid2col: inner.oid2column.clone(),
        }));
        Ok(ret)
    }

    pub fn schema(&self) -> Arc<SchemaTable> {
        self.inner.lock().unwrap().schema_table.clone()
    }
}

impl TableInner {
    pub fn new(table_schema: SchemaTable) -> RS<Self> {
        let key_indices = table_schema.key_indices().clone();
        let value_indices = table_schema.value_indices().clone();
        let (key_tuple_desc, key_tuple_payload_info) = table_schema.key_tuple_desc()?;
        let (value_tuple_desc, value_tuple_payload_info) = table_schema.value_tuple_desc()?;
        if value_tuple_desc.field_count() != value_tuple_payload_info.len() {
            return Err(m_error!(
                EC::DecodeErr,
                format!(
                    "field describe length mismatch: desc={} payload={}",
                    value_tuple_desc.field_count(),
                    value_tuple_payload_info.len()
                )
            ));
        }
        let mut name2oid = HashMap::new();
        let mut oid2column = HashMap::new();
        let mut fields = vec![FieldInfo::default(); table_schema.columns().len()];
        let mut key_oid = Vec::new();
        let mut value_oid = Vec::new();
        for field_info in key_tuple_payload_info {
            let column_index = field_info.column_index();
            key_oid.push(field_info.id());
            name2oid.insert(field_info.name().clone(), field_info.id());
            oid2column.insert(field_info.id(), field_info.clone());
            fields[column_index] = field_info;
        }
        for field_info in value_tuple_payload_info {
            let column_index = field_info.column_index();
            value_oid.push(field_info.id());
            name2oid.insert(field_info.name().clone(), field_info.id());
            oid2column.insert(field_info.id(), field_info.clone());
            fields[column_index] = field_info;
        }

        Ok(Self {
            schema_table: Arc::new(table_schema),
            name2oid,
            oid2column,
            fields,
            key_oid,
            value_oid,
            key_indices,
            value_indices,
            key_tuple_desc,
            value_tuple_desc,
        })
    }

    pub fn id(&self) -> OID {
        self.schema_table.id()
    }

    pub fn name(&self) -> &String {
        self.schema_table.table_name()
    }
}
