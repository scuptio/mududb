use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::error::ErrorCode;
use std::sync::Arc;

use crate::contract::partition_rule::PartitionRuleDesc;
use crate::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use mudu::common::result::RS;

#[async_trait]
pub trait MetaMgr: Send + Sync {
    async fn initialize(&self) -> RS<()>;
    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>>;

    async fn get_table_by_name(&self, name: &str) -> RS<Option<Arc<TableDesc>>>;

    async fn create_table(&self, schema: &SchemaTable) -> RS<()>;

    async fn drop_table(&self, table_id: OID) -> RS<()>;

    async fn create_partition_rule(&self, _rule: &PartitionRuleDesc) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "partition rule catalog is not implemented"
        ))
    }

    async fn get_partition_rule_by_id(&self, oid: OID) -> RS<PartitionRuleDesc> {
        Err(mudu::mudu_error!(
            ErrorCode::EntityNotFound,
            format!("no such partition rule {}", oid)
        ))
    }

    async fn get_partition_rule_by_name(&self, _name: &str) -> RS<Option<PartitionRuleDesc>> {
        Ok(None)
    }

    async fn list_partition_rules(&self) -> RS<Vec<PartitionRuleDesc>> {
        Ok(Vec::new())
    }

    async fn bind_table_partition(&self, _binding: &TablePartitionBinding) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "table partition binding is not implemented"
        ))
    }

    async fn get_table_partition_binding(
        &self,
        _table_id: OID,
    ) -> RS<Option<TablePartitionBinding>> {
        Ok(None)
    }

    async fn upsert_partition_placements(&self, _placements: &[PartitionPlacement]) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "partition placement is not implemented"
        ))
    }

    async fn get_partition_worker(&self, _partition_id: OID) -> RS<Option<OID>> {
        Ok(None)
    }

    async fn list_partition_placements(&self) -> RS<Vec<PartitionPlacement>> {
        Ok(Vec::new())
    }

    async fn list_schemas(&self) -> RS<Vec<SchemaTable>> {
        Ok(Vec::new())
    }
}
