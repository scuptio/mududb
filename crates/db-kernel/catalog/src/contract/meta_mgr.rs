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
