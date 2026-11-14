use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::error::ErrorCode;
use std::sync::Arc;

use crate::contract::fs_type::{FsTypeDesc, FsTypeKind};
use crate::contract::partition_rule::PartitionRuleDesc;
use crate::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use mudu::common::result::RS;

/// Catalog metadata manager: table schemas, partition
/// rules/bindings/placements, and filesystem types.
#[async_trait]
pub trait MetaMgr: Send + Sync {
    /// Initializes the manager, loading or creating the catalog.
    async fn initialize(&self) -> RS<()>;

    /// Flushes dirty data pages of the meta catalog relations
    /// (WAL-first deferred page flush). The default is a no-op for
    /// managers without persistent catalog files.
    async fn flush_dirty_pages(&self) -> RS<()> {
        Ok(())
    }

    /// Monotonic catalog (schema) version. Bumped once per applied DDL change
    /// (create/drop table, partition rule/binding/placement, fs-type), so plan
    /// caches can compare versions to detect schema invalidation. The default
    /// returns 0 for managers without version tracking (e.g. test mocks).
    fn catalog_version(&self) -> u64 {
        0
    }

    /// Resolves a table descriptor by table oid; errors when absent.
    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>>;

    /// Resolve a table by schema and bare name through the search path
    /// `[schema, mududb]`: an exact `(schema, name)` hit wins; on a miss (and
    /// when `schema` is not the default schema itself) the lookup falls back
    /// to the default schema, so tables created without an application
    /// context stay reachable from every application.
    async fn get_table_by_name(&self, schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>>;

    /// Resolve a table by exact `(schema, name)` with no search-path
    /// fallback. Used by DDL collision checks and by qualified
    /// (`schema.table`) references, both of which are exact-hit semantics.
    /// The default filters the search-path result by schema; managers with
    /// scoped name maps override it with a direct lookup.
    async fn get_table_exact(&self, schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        match self.get_table_by_name(schema, name).await? {
            Some(desc) if desc.schema() == schema => Ok(Some(desc)),
            _ => Ok(None),
        }
    }

    /// Creates a table from its schema.
    async fn create_table(&self, schema: &SchemaTable) -> RS<()>;

    /// Drops the table with the given oid.
    async fn drop_table(&self, table_id: OID) -> RS<()>;

    /// Creates a queue from its schema, which shall carry
    /// `SchemaObjectKind::Queue`. Queues share the `(schema, name)` namespace
    /// with tables. The default fails with `NotImplemented`.
    async fn create_queue(&self, _schema: &SchemaTable) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "queue catalog is not implemented"
        ))
    }

    /// Drops the queue with the given oid. The default fails with
    /// `NotImplemented`.
    async fn drop_queue(&self, _queue_id: OID) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "queue catalog is not implemented"
        ))
    }

    /// Resolves a queue by schema and name with the same search-path
    /// semantics as [`MetaMgr::get_table_by_name`]; a name that resolves to a
    /// non-queue object reports `None`. The default returns `None`.
    async fn get_queue(&self, _schema: &str, _name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(None)
    }

    /// Lists the schemas of all queues. The default returns an empty list.
    async fn list_queues(&self) -> RS<Vec<SchemaTable>> {
        Ok(Vec::new())
    }

    /// Creates a partition rule. The default fails with `NotImplemented`.
    async fn create_partition_rule(&self, _rule: &PartitionRuleDesc) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "partition rule catalog is not implemented"
        ))
    }

    /// Resolves a partition rule by oid. The default reports no such rule.
    async fn get_partition_rule_by_id(&self, oid: OID) -> RS<PartitionRuleDesc> {
        Err(mudu::mudu_error!(
            ErrorCode::EntityNotFound,
            format!("no such partition rule {}", oid)
        ))
    }

    /// Resolves a partition rule by name. The default returns `None`.
    async fn get_partition_rule_by_name(&self, _name: &str) -> RS<Option<PartitionRuleDesc>> {
        Ok(None)
    }

    /// Lists all partition rules. The default returns an empty list.
    async fn list_partition_rules(&self) -> RS<Vec<PartitionRuleDesc>> {
        Ok(Vec::new())
    }

    /// Binds a table to a partition rule. The default fails with
    /// `NotImplemented`.
    async fn bind_table_partition(&self, _binding: &TablePartitionBinding) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "table partition binding is not implemented"
        ))
    }

    /// Returns the partition binding of a table. The default returns `None`.
    async fn get_table_partition_binding(
        &self,
        _table_id: OID,
    ) -> RS<Option<TablePartitionBinding>> {
        Ok(None)
    }

    /// Lists all table partition bindings. The default returns an empty list.
    async fn list_partition_bindings(&self) -> RS<Vec<TablePartitionBinding>> {
        Ok(Vec::new())
    }

    /// Inserts or updates partition placements. The default fails with
    /// `NotImplemented`.
    async fn upsert_partition_placements(&self, _placements: &[PartitionPlacement]) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "partition placement is not implemented"
        ))
    }

    /// Returns the worker hosting a partition. The default returns `None`.
    async fn get_partition_worker(&self, _partition_id: OID) -> RS<Option<OID>> {
        Ok(None)
    }

    /// Lists all partition placements. The default returns an empty list.
    async fn list_partition_placements(&self) -> RS<Vec<PartitionPlacement>> {
        Ok(Vec::new())
    }

    /// Lists all table schemas. The default returns an empty list.
    async fn list_schemas(&self) -> RS<Vec<SchemaTable>> {
        Ok(Vec::new())
    }

    /// Creates a filesystem type and returns its id. The default fails with
    /// `NotImplemented`.
    async fn create_fs_type(&self, _name: &str, _kind: FsTypeKind) -> RS<u64> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "filesystem type catalog is not implemented"
        ))
    }

    /// Resolves a filesystem type by name. The default returns `None`.
    async fn get_fs_type_by_name(&self, _name: &str) -> RS<Option<FsTypeDesc>> {
        Ok(None)
    }

    /// Resolves a filesystem type by id. The default returns `None`.
    async fn get_fs_type_by_id(&self, _fs_id: u64) -> RS<Option<FsTypeDesc>> {
        Ok(None)
    }

    /// Lists all filesystem types. The default returns an empty list.
    async fn list_fs_types(&self) -> RS<Vec<FsTypeDesc>> {
        Ok(Vec::new())
    }

    /// Drops a filesystem type by name. The default fails with
    /// `NotImplemented`.
    async fn drop_fs_type(&self, _name: &str) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "filesystem type catalog is not implemented"
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use mudu::mudu_error;

    /// Minimal manager: required methods only, every default left as-is.
    struct DefaultMetaMgr;

    #[async_trait]
    impl MetaMgr for DefaultMetaMgr {
        async fn initialize(&self) -> RS<()> {
            Ok(())
        }
        async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
            Err(mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no table {oid}")
            ))
        }
        async fn get_table_by_name(
            &self,
            _schema: &str,
            _name: &str,
        ) -> RS<Option<Arc<TableDesc>>> {
            Ok(None)
        }
        async fn create_table(&self, _schema: &SchemaTable) -> RS<()> {
            Ok(())
        }
        async fn drop_table(&self, _table_id: OID) -> RS<()> {
            Ok(())
        }
    }

    #[test]
    fn default_methods_cover_noop_and_not_implemented_surface() {
        let meta = DefaultMetaMgr;
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            // No-op / empty defaults.
            meta.flush_dirty_pages().await.unwrap();
            assert_eq!(meta.catalog_version(), 0);
            assert!(meta.get_table_exact("s", "t").await.unwrap().is_none());
            assert!(meta
                .create_queue(&SchemaTable::new("q".to_string(), vec![], vec![], vec![]))
                .await
                .is_err());
            assert!(meta.drop_queue(1).await.is_err());
            assert!(meta.get_queue("s", "q").await.unwrap().is_none());
            assert!(meta.list_queues().await.unwrap().is_empty());
            assert!(meta.get_partition_rule_by_id(1).await.is_err());
            assert!(meta
                .get_partition_rule_by_name("r")
                .await
                .unwrap()
                .is_none());
            assert!(meta.list_partition_rules().await.unwrap().is_empty());
            assert!(meta.get_table_partition_binding(1).await.unwrap().is_none());
            assert!(meta.list_partition_bindings().await.unwrap().is_empty());
            assert!(meta.upsert_partition_placements(&[]).await.is_err());
            assert!(meta.get_partition_worker(1).await.unwrap().is_none());
            assert!(meta.list_partition_placements().await.unwrap().is_empty());
            assert!(meta.list_schemas().await.unwrap().is_empty());
            assert_eq!(
                meta.create_fs_type("bin", FsTypeKind::File)
                    .await
                    .unwrap_err()
                    .ec(),
                ErrorCode::NotImplemented
            );
            assert!(meta.get_fs_type_by_name("bin").await.unwrap().is_none());
            assert!(meta.get_fs_type_by_id(1).await.unwrap().is_none());
            assert!(meta.list_fs_types().await.unwrap().is_empty());
            assert!(meta.drop_fs_type("bin").await.is_err());
        })
        .unwrap();
    }
}
