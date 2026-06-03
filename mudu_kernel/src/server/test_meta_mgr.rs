use std::collections::HashMap;
use std::sync::Arc;
use mudu_sys::sync::SMutex;

use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;

pub(crate) struct TestMetaMgr {
    schemas: SMutex<HashMap<OID, SchemaTable>>,
    tables: SMutex<HashMap<OID, Arc<TableDesc>>>,
}

impl TestMetaMgr {
    pub(crate) fn new() -> Self {
        Self {
            schemas: SMutex::new(HashMap::new()),
            tables: SMutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl MetaMgr for TestMetaMgr {
    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
        self.tables
            .lock()
            .unwrap()
            .get(&oid)
            .cloned()
            .ok_or_else(|| m_error!(EC::NoSuchElement, format!("no such table {}", oid)))
    }

    async fn get_table_by_name(&self, name: &String) -> RS<Option<Arc<TableDesc>>> {
        Ok(self
            .tables
            .lock()
            .unwrap()
            .values()
            .find(|table| table.name() == name)
            .cloned())
    }

    async fn create_table(&self, schema: &SchemaTable) -> RS<()> {
        let table = TableInfo::new(schema.clone())?.table_desc()?;
        self.schemas
            .lock()
            .unwrap()
            .insert(schema.id(), schema.clone());
        self.tables.lock().unwrap().insert(schema.id(), table);
        Ok(())
    }

    async fn drop_table(&self, table_id: OID) -> RS<()> {
        self.schemas.lock().unwrap().remove(&table_id);
        self.tables.lock().unwrap().remove(&table_id);
        Ok(())
    }

    async fn list_schemas(&self) -> RS<Vec<SchemaTable>> {
        Ok(self.schemas.lock().unwrap().values().cloned().collect())
    }
}
