use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::error::ErrorCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::contract::fs_type::{FsTypeDesc, FsTypeKind};
use crate::contract::partition_rule::PartitionRuleDesc;
use crate::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use crate::contract::schema_class::SchemaClass;
use crate::contract::table_desc::TableDesc;
use mudu::common::result::RS;

/// A complete, versioned snapshot of the catalog definitions. Built-in system
/// tables are omitted (added by code), so a data-plane node can fetch this and
/// rebuild its local catalog.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CatalogSnapshot {
    /// The catalog version at the snapshot cut.
    pub version: u64,
    /// User table and queue schemas (built-ins excluded).
    pub schemas: Vec<SchemaClass>,
    /// Partition rules.
    pub rules: Vec<PartitionRuleDesc>,
    /// Table partition bindings.
    pub bindings: Vec<TablePartitionBinding>,
    /// Partition placements.
    pub placements: Vec<PartitionPlacement>,
    /// Filesystem types.
    pub fs_types: Vec<FsTypeDesc>,
}

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
    async fn create_table(&self, schema: &SchemaClass) -> RS<()>;

    /// Drops the table with the given oid.
    async fn drop_table(&self, table_id: OID) -> RS<()>;

    /// Creates a queue from its schema, which shall carry
    /// `SchemaClassKind::Queue`. Queues share the `(schema, name)` namespace
    /// with tables. The default fails with `NotImplemented`.
    async fn create_queue(&self, _schema: &SchemaClass) -> RS<()> {
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

    /// Creates a volatile, non-replicated cache from its schema, which shall
    /// carry `SchemaClassKind::Cache` and mirror an existing table's shape.
    /// The default fails with `NotImplemented`.
    async fn create_cache(&self, _schema: &SchemaClass) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "cache catalog is not implemented"
        ))
    }

    /// Resolves a queue by schema and name with the same search-path
    /// semantics as [`MetaMgr::get_table_by_name`]; a name that resolves to a
    /// non-queue object reports `None`. The default returns `None`.
    async fn get_queue(&self, _schema: &str, _name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(None)
    }

    /// Lists the schemas of all queues. The default returns an empty list.
    async fn list_queues(&self) -> RS<Vec<SchemaClass>> {
        Ok(Vec::new())
    }

    /// Creates a partition rule. The default fails with `NotImplemented`.
    async fn create_partition_rule(&self, _rule: &PartitionRuleDesc) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "partition rule catalog is not implemented"
        ))
    }

    /// Replaces a partition rule's definition in place (same oid; split /
    /// merge bump `version`). The default fails with `NotImplemented`.
    async fn update_partition_rule(&self, _rule: &PartitionRuleDesc) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "partition rule update is not implemented"
        ))
    }

    /// Resolves a partition rule by oid. The default reports no such rule.
    async fn get_partition_rule_by_id(&self, oid: OID) -> RS<PartitionRuleDesc> {
        Err(mudu::mudu_error!(
            ErrorCode::ObjectNotFound,
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

    /// Finds the `(rule name, partition name)` of a partition oid by scanning
    /// the partition rules. Cluster routing resolves static placements
    /// through names (oids alone do not key the static topology); managers
    /// with an index may override with a direct lookup.
    async fn find_partition_names(&self, partition_id: OID) -> RS<Option<(String, String)>> {
        for rule in self.list_partition_rules().await? {
            for partition in &rule.partitions {
                if partition.partition_id == partition_id {
                    return Ok(Some((rule.name.clone(), partition.name.clone())));
                }
            }
        }
        Ok(None)
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

    /// Removes a durable partition binding left by an interrupted object drop.
    /// Must reject a still-existing table/queue and persist removal before
    /// publishing it. Repeated cleanup of an absent binding is a no-op.
    async fn remove_orphan_partition_binding(&self, _table_id: OID) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "orphan partition binding cleanup is not implemented"
        ))
    }

    /// Lists all table schemas. The default returns an empty list.
    async fn list_schemas(&self) -> RS<Vec<SchemaClass>> {
        Ok(Vec::new())
    }

    /// Creates a filesystem type using its name-derived ID. Implementations
    /// must reject ID collisions before writing. The default fails with
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

    /// Captures a complete, versioned snapshot of the catalog definitions. The
    /// default fails with `NotImplemented`.
    async fn capture_catalog(&self) -> RS<CatalogSnapshot> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "catalog snapshot is not implemented"
        ))
    }

    /// Rebuilds the catalog from a snapshot (a data-plane node joining the
    /// cluster). The default fails with `NotImplemented`.
    async fn restore_catalog(&self, _snapshot: &CatalogSnapshot) -> RS<()> {
        Err(mudu::mudu_error!(
            ErrorCode::NotImplemented,
            "catalog restore is not implemented"
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
                ErrorCode::ObjectNotFound,
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
        async fn create_table(&self, _schema: &SchemaClass) -> RS<()> {
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
                .create_queue(&SchemaClass::new("q".to_string(), vec![], vec![], vec![]))
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
