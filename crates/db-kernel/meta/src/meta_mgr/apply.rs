use std::sync::atomic::Ordering;
use std::sync::Arc;

use mudu::common::id::OID;
use mudu::common::result::RS;

use ::catalog::contract::fs_type::FsTypeDesc;
use ::catalog::contract::partition_rule::PartitionRuleDesc;
use ::catalog::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_info::TableInfo;

use super::{registry, MetaMgrImpl};

impl MetaMgrImpl {
    pub(super) fn apply_create_table_local(&self, schema: &SchemaTable) -> RS<()> {
        let table_id = schema.id();
        let key = (schema.schema().clone(), schema.table_name().clone());
        let table = TableInfo::new(schema.clone())?;
        let _ = self.table.insert_sync(key.clone(), table.clone());
        let _ = self.id2table.insert_sync(table_id, table);
        let _ = self.name2id.insert_sync(key, table_id);
        Ok(())
    }

    pub(super) fn apply_drop_table_local(&self, schema: &str, table_name: &str, oid: OID) {
        let key = (schema.to_string(), table_name.to_string());
        let _ = self.id2table.remove_sync(&oid);
        let _ = self.name2id.remove_sync(&key);
        let _ = self.table.remove_sync(&key);
    }

    pub(super) fn apply_create_partition_rule_local(&self, rule: &PartitionRuleDesc) {
        let _ = self.rule_name2id.insert_sync(rule.name.clone(), rule.oid);
        let _ = self.rule_by_id.insert_sync(rule.oid, rule.clone());
    }

    pub(super) fn apply_bind_table_partition_local(&self, binding: &TablePartitionBinding) {
        let _ = self
            .binding_by_table_id
            .insert_sync(binding.table_id, binding.clone());
    }

    pub(super) fn apply_partition_placement_local(&self, placement: &PartitionPlacement) {
        let _ = self
            .placement_by_partition_id
            .insert_sync(placement.partition_id, placement.worker_id);
    }

    pub(super) fn apply_create_fs_type_local(&self, desc: &FsTypeDesc) {
        let _ = self
            .next_fs_id
            .fetch_max(desc.fs_id().saturating_add(1), Ordering::Relaxed);
        let _ = self
            .fs_type_by_name
            .insert_sync(desc.name().to_string(), desc.clone());
        let _ = self.fs_type_by_id.insert_sync(desc.fs_id(), desc.clone());
    }

    pub(super) fn apply_drop_fs_type_local(&self, name: &str) {
        if let Some((_, desc)) = self.fs_type_by_name.remove_sync(name) {
            let _ = self.fs_type_by_id.remove_sync(&desc.fs_id());
        }
    }

    pub(super) fn broadcast_create(&self, schema: &SchemaTable) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            self.apply_create_table_local(schema)?;
            self.bump_catalog_version();
            return Ok(());
        }
        for mgr in peers {
            mgr.apply_create_table_local(schema)?;
            mgr.bump_catalog_version();
        }
        Ok(())
    }

    pub(super) fn broadcast_drop(&self, schema: &str, table_name: &str, oid: OID) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            self.apply_drop_table_local(schema, table_name, oid);
            self.bump_catalog_version();
            return Ok(());
        }
        for mgr in peers {
            mgr.apply_drop_table_local(schema, table_name, oid);
            mgr.bump_catalog_version();
        }
        Ok(())
    }

    pub(super) fn broadcast_create_partition_rule(&self, rule: &PartitionRuleDesc) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            self.apply_create_partition_rule_local(rule);
            self.bump_catalog_version();
            return Ok(());
        }
        for mgr in peers {
            mgr.apply_create_partition_rule_local(rule);
            mgr.bump_catalog_version();
        }
        Ok(())
    }

    pub(super) fn broadcast_bind_table_partition(&self, binding: &TablePartitionBinding) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            self.apply_bind_table_partition_local(binding);
            self.bump_catalog_version();
            return Ok(());
        }
        for mgr in peers {
            mgr.apply_bind_table_partition_local(binding);
            mgr.bump_catalog_version();
        }
        Ok(())
    }

    pub(super) fn broadcast_upsert_partition_placements(
        &self,
        placements: &[PartitionPlacement],
    ) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            for placement in placements {
                self.apply_partition_placement_local(placement);
            }
            self.bump_catalog_version();
            return Ok(());
        }
        for mgr in peers {
            for placement in placements {
                mgr.apply_partition_placement_local(placement);
            }
            mgr.bump_catalog_version();
        }
        Ok(())
    }

    pub(super) fn broadcast_create_fs_type(&self, desc: &FsTypeDesc) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            self.apply_create_fs_type_local(desc);
            self.bump_catalog_version();
            return Ok(());
        }
        for mgr in peers {
            mgr.apply_create_fs_type_local(desc);
            mgr.bump_catalog_version();
        }
        Ok(())
    }

    pub(super) fn broadcast_drop_fs_type(&self, name: &str) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            self.apply_drop_fs_type_local(name);
            self.bump_catalog_version();
            return Ok(());
        }
        for mgr in peers {
            mgr.apply_drop_fs_type_local(name);
            mgr.bump_catalog_version();
        }
        Ok(())
    }

    pub(super) fn peer_instances(&self) -> RS<Vec<Arc<MetaMgrImpl>>> {
        let mut guard = registry().lock()?;
        let peers = guard.entry(self.path.clone()).or_default();
        let mut live = Vec::with_capacity(peers.len());
        peers.retain(|weak| match weak.upgrade() {
            Some(peer) => {
                live.push(peer);
                true
            }
            None => false,
        });
        Ok(live)
    }
}
