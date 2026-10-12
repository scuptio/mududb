use std::sync::Arc;

use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;

use ::catalog::contract::fs_type::{FsTypeDesc, FsTypeKind};
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::partition_rule::PartitionRuleDesc;
use ::catalog::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;

use super::MetaMgrImpl;

#[async_trait]
impl MetaMgr for MetaMgrImpl {
    async fn initialize(&self) -> RS<()> {
        self.initialize_inner().await
    }

    async fn flush_dirty_pages(&self) -> RS<()> {
        let catalog = self.catalog.lock()?.clone();
        if let Some(catalog) = catalog {
            catalog.flush_dirty_pages().await?;
        }
        Ok(())
    }

    fn catalog_version(&self) -> u64 {
        self.catalog_version_inner()
    }

    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
        let opt = self.lookup_table_info_by_id(oid);
        match opt {
            Some(table) => table.table_desc(),
            None => Err(mudu_error!(
                ER::EntityNotFound,
                format!("no such table {}", oid)
            )),
        }
    }

    async fn get_table_by_name(&self, schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        self.lookup_table_by_name(schema, name)
    }

    async fn get_table_exact(&self, schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        self.lookup_table_exact(schema, name)
    }

    async fn create_table(&self, schema: &SchemaTable) -> RS<()> {
        self.create_table_inner(schema).await
    }

    async fn drop_table(&self, table_id: OID) -> RS<()> {
        self.drop_table_inner(table_id).await
    }

    async fn create_partition_rule(&self, rule: &PartitionRuleDesc) -> RS<()> {
        self.create_partition_rule_inner(rule).await
    }

    async fn get_partition_rule_by_id(&self, oid: OID) -> RS<PartitionRuleDesc> {
        self.lookup_partition_rule_by_id(oid).ok_or_else(|| {
            mudu_error!(
                ER::EntityNotFound,
                format!("no such partition rule {}", oid)
            )
        })
    }

    async fn get_partition_rule_by_name(&self, name: &str) -> RS<Option<PartitionRuleDesc>> {
        Ok(self.lookup_partition_rule_by_name(name))
    }

    async fn list_partition_rules(&self) -> RS<Vec<PartitionRuleDesc>> {
        Ok(self.list_partition_rules_inner())
    }

    async fn bind_table_partition(&self, binding: &TablePartitionBinding) -> RS<()> {
        self.bind_table_partition_inner(binding).await
    }

    async fn get_table_partition_binding(
        &self,
        table_id: OID,
    ) -> RS<Option<TablePartitionBinding>> {
        Ok(self.lookup_table_partition_binding(table_id))
    }

    async fn list_partition_bindings(&self) -> RS<Vec<TablePartitionBinding>> {
        Ok(self.list_partition_bindings_inner())
    }

    async fn upsert_partition_placements(&self, placements: &[PartitionPlacement]) -> RS<()> {
        self.upsert_partition_placements_inner(placements).await
    }

    async fn get_partition_worker(&self, partition_id: OID) -> RS<Option<OID>> {
        Ok(self
            .placement_by_partition_id
            .get_sync(&partition_id)
            .map(|entry| *entry.get()))
    }

    async fn list_partition_placements(&self) -> RS<Vec<PartitionPlacement>> {
        Ok(self.list_partition_placements_inner())
    }

    async fn list_schemas(&self) -> RS<Vec<SchemaTable>> {
        self.list_schemas_inner()
    }

    async fn create_fs_type(&self, name: &str, kind: FsTypeKind) -> RS<u64> {
        self.create_fs_type_inner(name, kind).await
    }

    async fn get_fs_type_by_name(&self, name: &str) -> RS<Option<FsTypeDesc>> {
        Ok(self.lookup_fs_type_by_name(name))
    }

    async fn get_fs_type_by_id(&self, fs_id: u64) -> RS<Option<FsTypeDesc>> {
        Ok(self.lookup_fs_type_by_id(fs_id))
    }

    async fn list_fs_types(&self) -> RS<Vec<FsTypeDesc>> {
        Ok(self.list_fs_types_inner())
    }

    async fn drop_fs_type(&self, name: &str) -> RS<()> {
        self.drop_fs_type_inner(name).await
    }
}
