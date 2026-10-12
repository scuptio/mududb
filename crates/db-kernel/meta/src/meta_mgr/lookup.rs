use std::sync::atomic::Ordering;
use std::sync::Arc;

use mudu::common::id::OID;
use mudu::common::result::RS;

use ::catalog::contract::app_schema::DEFAULT_SCHEMA;
use ::catalog::contract::fs_type::FsTypeDesc;
use ::catalog::contract::partition_rule::PartitionRuleDesc;
use ::catalog::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;

use super::{now_catalog_xid, MetaMgrImpl};

impl MetaMgrImpl {
    pub(crate) fn lookup_table_info_by_id(&self, oid: OID) -> Option<TableInfo> {
        let opt = self.id2table.get_sync(&oid);
        opt.map(|entry| entry.get().clone())
    }

    pub(crate) fn lookup_table_by_name(
        &self,
        schema: &str,
        name: &str,
    ) -> RS<Option<Arc<TableDesc>>> {
        let key = (schema.to_string(), name.to_string());
        let opt = self.table.get_sync(&key);
        let table = match opt {
            Some(table) => Some(table.get().clone()),
            // Search path fallback: an unqualified lookup that misses in the
            // current schema resolves against the default schema.
            None if schema != DEFAULT_SCHEMA => self
                .table
                .get_sync(&(DEFAULT_SCHEMA.to_string(), name.to_string()))
                .map(|table| table.get().clone()),
            None => None,
        };
        let table_desc = match table {
            None => return Ok(None),
            Some(table) => table.table_desc()?,
        };
        Ok(Some(table_desc))
    }

    pub(crate) fn lookup_table_exact(
        &self,
        schema: &str,
        name: &str,
    ) -> RS<Option<Arc<TableDesc>>> {
        let opt = self.table.get_sync(&(schema.to_string(), name.to_string()));
        let table_desc = match opt {
            None => return Ok(None),
            Some(table) => table.get().table_desc()?,
        };
        Ok(Some(table_desc))
    }

    pub(crate) fn list_schemas_inner(&self) -> RS<Vec<SchemaTable>> {
        let mut schemas = Vec::new();
        self.table.iter_sync(|_table_name, table_info| {
            if let Ok(schema) = table_info.schema() {
                schemas.push(schema.as_ref().clone());
            }
            true
        });
        schemas.sort_by_key(|schema| schema.id());
        Ok(schemas)
    }

    pub(crate) fn lookup_partition_rule_by_id(&self, oid: OID) -> Option<PartitionRuleDesc> {
        self.rule_by_id
            .get_sync(&oid)
            .map(|entry| entry.get().clone())
    }

    pub(crate) fn lookup_partition_rule_by_name(&self, name: &str) -> Option<PartitionRuleDesc> {
        let rule_id = self.rule_name2id.get_sync(name).map(|entry| *entry.get())?;
        self.lookup_partition_rule_by_id(rule_id)
    }

    pub(crate) fn list_partition_rules_inner(&self) -> Vec<PartitionRuleDesc> {
        let mut rules = Vec::new();
        self.rule_by_id.iter_sync(|_rule_id, rule| {
            rules.push(rule.clone());
            true
        });
        rules.sort_by_key(|rule| rule.oid);
        rules
    }

    pub(crate) fn lookup_table_partition_binding(
        &self,
        table_id: OID,
    ) -> Option<TablePartitionBinding> {
        self.binding_by_table_id
            .get_sync(&table_id)
            .map(|entry| entry.get().clone())
    }

    pub(crate) fn list_partition_bindings_inner(&self) -> Vec<TablePartitionBinding> {
        let mut bindings = Vec::new();
        self.binding_by_table_id.iter_sync(|_table_id, binding| {
            bindings.push(binding.clone());
            true
        });
        bindings.sort_by_key(|binding| binding.table_id);
        bindings
    }

    pub(crate) fn list_partition_placements_inner(&self) -> Vec<PartitionPlacement> {
        let mut placements = Vec::new();
        self.placement_by_partition_id
            .iter_sync(|partition_id, worker_id| {
                placements.push(PartitionPlacement {
                    partition_id: *partition_id,
                    worker_id: *worker_id,
                });
                true
            });
        placements.sort_by_key(|placement| placement.partition_id);
        placements
    }

    pub(crate) fn lookup_fs_type_by_name(&self, name: &str) -> Option<FsTypeDesc> {
        self.fs_type_by_name.read_sync(name, |_, desc| desc.clone())
    }

    pub(crate) fn lookup_fs_type_by_id(&self, fs_id: u64) -> Option<FsTypeDesc> {
        self.fs_type_by_id.read_sync(&fs_id, |_, desc| desc.clone())
    }

    pub(crate) fn list_fs_types_inner(&self) -> Vec<FsTypeDesc> {
        let mut fs_types = Vec::new();
        self.fs_type_by_id.iter_sync(|_fs_id, desc| {
            fs_types.push(desc.clone());
            true
        });
        fs_types.sort_by_key(|desc| desc.fs_id());
        fs_types
    }

    /// Loads the current catalog (schema) version with acquire semantics so
    /// that a version check also observes the schema changes published before
    /// the corresponding release-increment.
    pub(crate) fn catalog_version_inner(&self) -> u64 {
        self.catalog_version.load(Ordering::Acquire)
    }

    /// Publishes one applied DDL change by bumping the catalog version. Called
    /// once per instance per logical DDL, including instances reached through
    /// the registry fan-out, so all instances on the same path advance in
    /// lockstep. Catalog replay during `initialize_inner` does not bump it.
    pub(super) fn bump_catalog_version(&self) {
        self.catalog_version.fetch_add(1, Ordering::Release);
    }

    pub(super) fn next_catalog_xid(&self) -> u64 {
        let mut next = self.next_catalog_xid.load(Ordering::Relaxed);
        loop {
            let candidate = now_catalog_xid().max(next.saturating_add(1));
            match self.next_catalog_xid.compare_exchange(
                next,
                candidate,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return candidate,
                Err(actual) => next = actual,
            }
        }
    }
}
