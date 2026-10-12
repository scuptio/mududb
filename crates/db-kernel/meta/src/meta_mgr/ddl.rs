use std::sync::atomic::Ordering;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use tracing::trace;

use crate::fs_type_catalog::{delete_fs_type_from_catalog, write_fs_type_to_catalog};
use crate::partition_binding_catalog::write_partition_binding_to_catalog;
use crate::partition_placement_catalog::write_partition_placement_to_catalog;
use crate::partition_rule_catalog::write_partition_rule_to_catalog;
use crate::schema_catalog::{delete_schema_from_catalog, write_schema_to_catalog};
use ::catalog::contract::fs_type::{FsTypeDesc, FsTypeKind};
use ::catalog::contract::partition_rule::PartitionRuleDesc;
use ::catalog::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use ::catalog::contract::schema_table::SchemaTable;

use super::MetaMgrImpl;

impl MetaMgrImpl {
    pub(crate) async fn create_table_inner(&self, schema: &SchemaTable) -> RS<()> {
        trace!(table = %schema.table_name(), oid = schema.id(), "meta_mgr create_table_inner start");
        let _ddl_guard = self.ddl_lock.lock().await;
        trace!(table = %schema.table_name(), oid = schema.id(), "meta_mgr create_table_inner acquired ddl lock");
        // The collision check is scoped to (schema, name): two applications
        // may own same-named tables in their own namespaces.
        if self
            .table
            .contains_sync(&(schema.schema().clone(), schema.table_name().clone()))
        {
            return Err(mudu_error!(ER::EntityAlreadyExists, ""));
        }

        trace!(table = %schema.table_name(), oid = schema.id(), "meta_mgr writing schema to catalog");
        let schema_catalog = self.catalog_relation()?.schema_catalog;
        write_schema_to_catalog(&schema_catalog, schema, self.next_catalog_xid()).await?;
        trace!(table = %schema.table_name(), oid = schema.id(), "meta_mgr wrote schema to catalog");
        let r = self.broadcast_create(schema);
        trace!(table = %schema.table_name(), oid = schema.id(), "meta_mgr broadcast create done");
        r
    }

    pub(crate) async fn drop_table_inner(&self, oid: OID) -> RS<()> {
        let _ddl_guard = self.ddl_lock.lock().await;
        let table = self
            .lookup_table_info_by_id(oid)
            .ok_or_else(|| mudu_error!(ER::EntityNotFound, format!("no such table {}", oid)))?;
        let schema_catalog = self.catalog_relation()?.schema_catalog;

        delete_schema_from_catalog(&schema_catalog, oid, self.next_catalog_xid()).await?;
        let schema = table.schema()?;
        self.broadcast_drop(schema.schema(), schema.table_name(), oid)
    }

    pub(crate) async fn create_partition_rule_inner(&self, rule: &PartitionRuleDesc) -> RS<()> {
        let _ddl_guard = self.ddl_lock.lock().await;
        if self.rule_name2id.contains_sync(&rule.name) {
            return Err(mudu_error!(
                ER::EntityAlreadyExists,
                format!("partition rule {} already exists", rule.name)
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
        if self.lookup_table_info_by_id(binding.table_id).is_none() {
            return Err(mudu_error!(
                ER::EntityNotFound,
                format!("no such table {}", binding.table_id)
            ));
        }
        if self.lookup_partition_rule_by_id(binding.rule_id).is_none() {
            return Err(mudu_error!(
                ER::EntityNotFound,
                format!("no such partition rule {}", binding.rule_id)
            ));
        }
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
        let fs_id = self.next_fs_id.fetch_add(1, Ordering::Relaxed);
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
                ER::EntityNotFound,
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
