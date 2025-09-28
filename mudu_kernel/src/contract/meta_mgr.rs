use async_trait::async_trait;
use mudu::common::id::OID;
use std::sync::Arc;

use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use mudu::common::result::RS;

#[async_trait]
pub trait MetaMgr: Send + Sync {
    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>>;

    async fn get_table_by_name(&self, name: &String) -> RS<Option<Arc<TableDesc>>>;

    async fn create_table(&self, schema: &SchemaTable) -> RS<()>;

    async fn drop_table(&self, table_id: OID) -> RS<()>;
}
