use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use tracing::trace;

use crate::fs_type_catalog::{delete_fs_type_from_catalog, write_fs_type_to_catalog};
use crate::partition_binding_catalog::{
    delete_partition_binding_from_catalog, write_partition_binding_to_catalog,
};
use crate::partition_placement_catalog::write_partition_placement_to_catalog;
use crate::partition_rule_catalog::write_partition_rule_to_catalog;
use crate::schema_catalog::{delete_schema_from_catalog, write_schema_to_catalog};
use ::catalog::contract::fs_type::{FsTypeDesc, FsTypeKind};
use ::catalog::contract::partition_rule::PartitionRuleDesc;
use ::catalog::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use ::catalog::contract::schema_class::{SchemaClass, SchemaClassKind};

use super::MetaMgrImpl;

impl MetaMgrImpl {
    pub(crate) async fn create_table_inner(&self, schema: &SchemaClass) -> RS<()> {
        trace!(table = %schema.class_name(), oid = schema.id(), "meta_mgr create_table_inner start");
        let _ddl_guard = self.ddl_lock.lock().await;
        trace!(table = %schema.class_name(), oid = schema.id(), "meta_mgr create_table_inner acquired ddl lock");
        // The collision check is scoped to (schema, name): two applications
        // may own same-named tables in their own namespaces.
        if self
            .table
            .contains_sync(&(schema.schema().clone(), schema.class_name().clone()))
        {
            return Err(mudu_error!(ER::ObjectAlreadyExists, ""));
        }

        trace!(table = %schema.class_name(), oid = schema.id(), "meta_mgr writing schema to catalog");
        if self.id2table.contains_sync(&schema.id()) {
            return Err(mudu_error!(
                ER::ObjectAlreadyExists,
                "catalog object OID already exists"
            ));
        }
        super::references::validate_fs_references(schema, |id| self.lookup_fs_type_by_id(id))?;
        let schema_catalog = self.catalog_relation()?.schema_catalog;
        write_schema_to_catalog(&schema_catalog, schema, self.next_catalog_xid()).await?;
        trace!(table = %schema.class_name(), oid = schema.id(), "meta_mgr wrote schema to catalog");
        let r = self.broadcast_create(schema);
        trace!(table = %schema.class_name(), oid = schema.id(), "meta_mgr broadcast create done");
        r
    }

    pub(crate) async fn drop_table_inner(&self, oid: OID) -> RS<()> {
        let _ddl_guard = self.ddl_lock.lock().await;
        let table = self
            .lookup_table_info_by_id(oid)
            .ok_or_else(|| mudu_error!(ER::ObjectNotFound, format!("no such table {}", oid)))?;
        let schema_catalog = self.catalog_relation()?.schema_catalog;

        delete_schema_from_catalog(&schema_catalog, oid, self.next_catalog_xid()).await?;
        self.delete_partition_binding(oid).await?;
        let schema = table.schema()?;
        self.broadcast_drop(schema.schema(), schema.class_name(), oid)
    }

    pub(crate) async fn create_queue_inner(&self, schema: &SchemaClass) -> RS<()> {
        if schema.kind() != SchemaClassKind::Queue {
            return Err(mudu_error!(
                ER::InvalidArgument,
                format!(
                    "schema {} does not carry the queue kind",
                    schema.class_name()
                )
            ));
        }
        let _ddl_guard = self.ddl_lock.lock().await;
        // Queues share the (schema, name) namespace with tables: a same-named
        // table or queue in the same schema is a collision.
        if self
            .table
            .contains_sync(&(schema.schema().clone(), schema.class_name().clone()))
        {
            return Err(mudu_error!(
                ER::ObjectAlreadyExists,
                format!("queue {} already exists", schema.class_name())
            ));
        }

        if self.id2table.contains_sync(&schema.id()) {
            return Err(mudu_error!(
                ER::ObjectAlreadyExists,
                "catalog object OID already exists"
            ));
        }
        super::references::validate_fs_references(schema, |id| self.lookup_fs_type_by_id(id))?;
        let schema_catalog = self.catalog_relation()?.schema_catalog;
        write_schema_to_catalog(&schema_catalog, schema, self.next_catalog_xid()).await?;
        // Reuses the table broadcast: the local apply is identical for every
        // SchemaClass, and it bumps the catalog version so plan caches
        // invalidate.
        self.broadcast_create(schema)
    }

    pub(crate) async fn create_cache_inner(&self, schema: &SchemaClass) -> RS<()> {
        if schema.kind() != SchemaClassKind::Cache {
            return Err(mudu_error!(
                ER::InvalidArgument,
                format!(
                    "schema {} does not carry the cache kind",
                    schema.class_name()
                )
            ));
        }
        // A cache is a volatile, non-replicated catalog object: it registers
        // the schema (mirroring the `LIKE` table) but never creates storage,
        // and it is excluded from replication/snapshot. The catalog row itself
        // is durable only so the declaration survives a restart for
        // rebuild-on-warmup; its contents are always reconstructed from the
        // authoritative table.
        let _ddl_guard = self.ddl_lock.lock().await;
        if self
            .table
            .contains_sync(&(schema.schema().clone(), schema.class_name().clone()))
        {
            return Err(mudu_error!(
                ER::ObjectAlreadyExists,
                format!("cache {} already exists", schema.class_name())
            ));
        }
        if self.id2table.contains_sync(&schema.id()) {
            return Err(mudu_error!(
                ER::ObjectAlreadyExists,
                "catalog object OID already exists"
            ));
        }
        super::references::validate_fs_references(schema, |id| self.lookup_fs_type_by_id(id))?;
        let schema_catalog = self.catalog_relation()?.schema_catalog;
        write_schema_to_catalog(&schema_catalog, schema, self.next_catalog_xid()).await?;
        self.broadcast_create(schema)
    }

    pub(crate) async fn drop_queue_inner(&self, oid: OID) -> RS<()> {
        let _ddl_guard = self.ddl_lock.lock().await;
        let table = self
            .lookup_table_info_by_id(oid)
            .ok_or_else(|| mudu_error!(ER::ObjectNotFound, format!("no such queue {}", oid)))?;
        let schema = table.schema()?;
        if schema.kind() != SchemaClassKind::Queue {
            return Err(mudu_error!(
                ER::ObjectNotFound,
                format!("no such queue {}", oid)
            ));
        }
        let schema_catalog = self.catalog_relation()?.schema_catalog;

        delete_schema_from_catalog(&schema_catalog, oid, self.next_catalog_xid()).await?;
        self.delete_partition_binding(oid).await?;
        self.broadcast_drop(schema.schema(), schema.class_name(), oid)
    }

    // Called with ddl_lock held. Schema deletion is persisted first so an
    // interruption never resurrects an existing table without its routing.
    async fn delete_partition_binding(&self, oid: OID) -> RS<()> {
        if self.binding_by_table_id.contains_sync(&oid) {
            let relation = self.catalog_relation()?.partition_binding_catalog;
            delete_partition_binding_from_catalog(&relation, oid, self.next_catalog_xid()).await?;
        }
        Ok(())
    }

    pub(crate) async fn remove_orphan_partition_binding_inner(&self, oid: OID) -> RS<()> {
        let _ddl_guard = self.ddl_lock.lock().await;
        if self.lookup_table_info_by_id(oid).is_some() {
            return Err(mudu_error!(
                ER::InvalidState,
                "cannot clean binding of an existing object"
            ));
        }
        if !self.binding_by_table_id.contains_sync(&oid) {
            return Ok(());
        }
        self.delete_partition_binding(oid).await?;
        self.broadcast_remove_partition_binding(oid)
    }

    /// Split/merge path: overwrite the rule's catalog row and refresh the
    /// in-memory copies (same oid key; the caller bumped the version).
    pub(crate) async fn update_partition_rule_inner(&self, rule: &PartitionRuleDesc) -> RS<()> {
        let _ddl_guard = self.ddl_lock.lock().await;
        let existing = self
            .lookup_partition_rule_by_id(rule.oid)
            .ok_or_else(|| mudu_error!(ER::ObjectNotFound, "partition rule OID does not exist"))?;
        if existing.name != rule.name {
            return Err(mudu_error!(
                ER::InvalidState,
                "partition rule identity changed"
            ));
        }
        if existing == *rule {
            return Ok(());
        }
        if existing.version.checked_add(1) != Some(rule.version) {
            return Err(mudu_error!(
                ER::InvalidState,
                "partition rule update is not the next version"
            ));
        }
        self.validate_rule_bindings(rule)?;
        let partition_rule_catalog = self.catalog_relation()?.partition_rule_catalog;
        write_partition_rule_to_catalog(&partition_rule_catalog, rule, self.next_catalog_xid())
            .await?;
        self.broadcast_create_partition_rule(rule)
    }

    pub(crate) async fn create_partition_rule_inner(&self, rule: &PartitionRuleDesc) -> RS<()> {
        let _ddl_guard = self.ddl_lock.lock().await;
        if self.rule_name2id.contains_sync(&rule.name) {
            return Err(mudu_error!(
                ER::ObjectAlreadyExists,
                format!("partition rule {} already exists", rule.name)
            ));
        }
        if self.rule_by_id.contains_sync(&rule.oid) {
            return Err(mudu_error!(
                ER::ObjectAlreadyExists,
                "partition rule OID already exists"
            ));
        }
        let partition_rule_catalog = self.catalog_relation()?.partition_rule_catalog;

        write_partition_rule_to_catalog(&partition_rule_catalog, rule, self.next_catalog_xid())
            .await?;
        self.broadcast_create_partition_rule(rule)
    }

    pub(crate) async fn bind_table_partition_inner(
        &self,
        binding: &TablePartitionBinding,
    ) -> RS<()> {
        let _ddl_guard = self.ddl_lock.lock().await;
        let table = self
            .lookup_table_info_by_id(binding.table_id)
            .ok_or_else(|| {
                mudu_error!(ER::ObjectNotFound, "partition binding table does not exist")
            })?;
        let rule = self
            .lookup_partition_rule_by_id(binding.rule_id)
            .ok_or_else(|| {
                mudu_error!(ER::ObjectNotFound, "partition binding rule does not exist")
            })?;
        binding.validate(table.schema()?.as_ref(), &rule)?;
        let partition_binding_catalog = self.catalog_relation()?.partition_binding_catalog;

        write_partition_binding_to_catalog(
            &partition_binding_catalog,
            binding,
            self.next_catalog_xid(),
        )
        .await?;
        self.broadcast_bind_table_partition(binding)
    }

    pub(crate) async fn upsert_partition_placements_inner(
        &self,
        placements: &[PartitionPlacement],
    ) -> RS<()> {
        let _ddl_guard = self.ddl_lock.lock().await;
        let partition_placement_catalog = self.catalog_relation()?.partition_placement_catalog;

        for placement in placements {
            write_partition_placement_to_catalog(
                &partition_placement_catalog,
                placement,
                self.next_catalog_xid(),
            )
            .await?;
        }
        self.broadcast_upsert_partition_placements(placements)
    }

    pub(crate) async fn create_fs_type_inner(&self, name: &str, kind: FsTypeKind) -> RS<u64> {
        let _ddl_guard = self.ddl_lock.lock().await;
        if self.fs_type_by_name.contains_sync(name) {
            return Err(mudu_error!(
                ER::AlreadyExists,
                format!("filesystem type {} already exists", name)
            ));
        }
        let fs_id = catalog::contract::derive_oid::derive_fs_id(name);
        if self.fs_type_by_id.contains_sync(&fs_id) {
            return Err(mudu_error!(
                ER::InvalidState,
                "filesystem type ID collision"
            ));
        }
        let desc = FsTypeDesc::new(name.to_string(), fs_id, kind);
        let fs_type_catalog = self.catalog_relation()?.fs_type;

        write_fs_type_to_catalog(&fs_type_catalog, &desc, self.next_catalog_xid()).await?;
        self.broadcast_create_fs_type(&desc)?;
        Ok(fs_id)
    }

    pub(crate) async fn drop_fs_type_inner(&self, name: &str) -> RS<()> {
        let _ddl_guard = self.ddl_lock.lock().await;
        let target = self.lookup_fs_type_by_name(name).ok_or_else(|| {
            mudu_error!(
                ER::ObjectNotFound,
                format!("no such filesystem type {}", name)
            )
        })?;
        self.check_fs_type_not_referenced(&target)?;
        let fs_type_catalog = self.catalog_relation()?.fs_type;

        delete_fs_type_from_catalog(&fs_type_catalog, name, self.next_catalog_xid()).await?;
        self.broadcast_drop_fs_type(name)
    }

    // Reject dropping a filesystem type that is still referenced by an
    // FS-bound table column (SchemaColumn::fs_binding).
    fn check_fs_type_not_referenced(&self, desc: &FsTypeDesc) -> RS<()> {
        let mut result = Ok(());
        self.table.iter_sync(|key, table_info| {
            let schema = match table_info.schema() {
                Ok(schema) => schema,
                Err(e) => {
                    result = Err(e);
                    return false;
                }
            };
            for column in schema.columns() {
                if let Some(binding) = column.fs_binding() {
                    if binding.fs_id() == desc.fs_id() {
                        result = Err(mudu_error!(
                            ER::InvalidState,
                            format!(
                                "fs type {} still referenced by column {}.{}",
                                desc.name(),
                                key.1,
                                column.get_name()
                            )
                        ));
                        return false;
                    }
                }
            }
            true
        });
        result
    }
}
