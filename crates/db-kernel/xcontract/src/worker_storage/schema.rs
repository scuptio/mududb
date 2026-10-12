use std::collections::BTreeSet;
use std::sync::Arc;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::{scoped_task_trace, task_trace};
use tracing::{info, trace};

use ::catalog::contract::partition_rule_binding::TablePartitionBinding;
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;
use ::meta::fs_object::{fs_object_desc, FS_OBJECT_TABLE_ID};
use ::storage::relation::relation::Relation;
use ::xengine::partition_router::DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID;
use ::xengine::tx_mgr::PhysicalRelationId;

use super::{collect_bound_partition_ids, WorkerStorage};

impl WorkerStorage {
    pub(crate) async fn bootstrap_existing_tables_async(&self) -> RS<()> {
        let schemas = self.mgr.list_schemas().await?;
        for schema in &schemas {
            self.bootstrap_table_local_async(schema).await?;
        }
        self.bootstrap_fs_object_relations_async(&schemas).await?;
        Ok(())
    }

    // Open one `_fs_object` relation per partition known to the local meta
    // (plus partition 0) so fs-object rows staged by the DML hooks can be
    // committed and replayed on any local partition.
    async fn bootstrap_fs_object_relations_async(&self, schemas: &[SchemaTable]) -> RS<()> {
        let mut partition_ids = BTreeSet::new();
        partition_ids.insert(DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID);
        collect_bound_partition_ids(&self.mgr, schemas, &mut partition_ids).await?;
        let table_desc = fs_object_desc()?;
        for partition_id in partition_ids {
            let relation_id = self.relation_id(FS_OBJECT_TABLE_ID, partition_id);
            if self.relation_store.contains_async(&relation_id).await {
                continue;
            }
            self.create_relation_index_for_partition_async(
                FS_OBJECT_TABLE_ID,
                partition_id,
                table_desc.as_ref(),
            )
            .await?;
        }
        Ok(())
    }

    pub(crate) async fn create_table_async(&self, schema: &SchemaTable) -> RS<()> {
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage create_table_async start");
        self.mgr.create_table(schema).await?;
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage metadata create finished");
        let r = self.broadcast_create_table_async(schema).await;
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage broadcast_create_table_async finished");
        r
    }

    pub(crate) async fn drop_table_async(&self, oid: OID) -> RS<()> {
        self.mgr.drop_table(oid).await?;
        self.broadcast_drop_table_async(oid).await
    }

    async fn create_relation_index_for_partition_async(
        &self,
        oid: OID,
        partition_id: OID,
        table_desc: &TableDesc,
    ) -> RS<()> {
        scoped_task_trace!();
        let relation_id = self.relation_id(oid, partition_id);
        // Double-checked under the creation lock: only one task ever opens a
        // relation's files, so an open can never scan a page chain while the
        // stored instance is mid-mutation.
        let _guard = self.relation_create_lock.lock().await;
        if self.relation_store.contains_async(&relation_id).await {
            return Ok(());
        }

        let relation = match &self.async_runtime {
            Some(async_runtime) => Arc::new(
                Relation::new_with_provider(
                    async_runtime.clone(),
                    oid,
                    partition_id,
                    self.relation_path.clone(),
                    table_desc,
                )
                .await?,
            ),
            None => Arc::new(
                Relation::new(oid, partition_id, self.relation_path.clone(), table_desc).await?,
            ),
        };

        let _ = self
            .relation_store
            .insert_async(relation_id, relation)
            .await;
        Ok(())
    }

    pub(super) async fn ensure_relation_index(
        &self,
        oid: OID,
        partition_id: Option<OID>,
    ) -> RS<()> {
        scoped_task_trace!();
        let partition_id = self.physical_partition_id(partition_id);
        let relation_id = self.relation_id(oid, partition_id);
        if self.relation_store.contains_async(&relation_id).await {
            return Ok(());
        }

        let table_desc = self.mgr.get_table_by_id(oid).await?;
        self.create_relation_index_for_partition_async(oid, partition_id, table_desc.as_ref())
            .await?;
        Ok(())
    }

    async fn apply_create_table_local_async(&self, schema: &SchemaTable) -> RS<()> {
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage apply_create_table_local_async start");
        let table_desc =
            ::catalog::contract::table_info::TableInfo::new(schema.clone())?.table_desc()?;
        self.create_relation_index_for_partition_async(
            schema.id(),
            DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
            table_desc.as_ref(),
        )
        .await?;
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage apply_create_table_local_async done");
        Ok(())
    }

    async fn bootstrap_table_local_async(&self, schema: &SchemaTable) -> RS<()> {
        let table_desc =
            ::catalog::contract::table_info::TableInfo::new(schema.clone())?.table_desc()?;
        let binding = self.mgr.get_table_partition_binding(schema.id()).await?;
        match binding {
            Some(binding) => {
                self.create_partitioned_relations_async(schema.id(), &binding, table_desc.as_ref())
                    .await
            }
            None => {
                self.create_relation_index_for_partition_async(
                    schema.id(),
                    DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
                    table_desc.as_ref(),
                )
                .await
            }
        }
    }

    async fn create_partitioned_relations_async(
        &self,
        oid: OID,
        binding: &TablePartitionBinding,
        table_desc: &TableDesc,
    ) -> RS<()> {
        let rule = self.mgr.get_partition_rule_by_id(binding.rule_id).await?;
        for partition in &rule.partitions {
            self.create_relation_index_for_partition_async(oid, partition.partition_id, table_desc)
                .await?;
        }
        Ok(())
    }

    async fn apply_drop_table_local_async(&self, oid: OID) {
        let trace = task_trace!();
        let task_id = mudu_sys::task::async_::try_this_task_id();
        let relation_id = self.relation_id(oid, self.default_partition_id);
        trace.watch("relation_store.op", "remove_async");
        trace.watch("relation_store.phase", "before_remove");
        trace.watch("relation_store.relation_id", &format!("{relation_id:?}"));
        trace.watch("relation_store.task_id", &format!("{task_id:?}"));
        let removed = self.relation_store.remove_async(&relation_id).await;
        trace.watch("relation_store.phase", "after_remove");
        info!(
            task_id = ?task_id,
            relation_id = ?relation_id,
            existed = removed.is_some(),
            "relation_store remove_async"
        );
    }

    async fn broadcast_create_table_async(&self, schema: &SchemaTable) -> RS<()> {
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage broadcast_create_table_async enter");
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            return self.apply_create_table_local_async(schema).await;
        }
        for storage in peers {
            storage.apply_create_table_local_async(schema).await?;
        }
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage broadcast_create_table_async done");
        Ok(())
    }

    /// Return the open relation index of table `oid` on `partition_id` (the
    /// default partition when `None`).
    pub async fn get_relation_async(
        &self,
        oid: OID,
        partition_id: Option<OID>,
    ) -> RS<Arc<Relation>> {
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        self.get_relation_by_id_async(&relation_id).await
    }

    pub(super) async fn get_relation_by_id_async(
        &self,
        relation_id: &PhysicalRelationId,
    ) -> RS<Arc<Relation>> {
        self.relation_store
            .get_async(relation_id)
            .await
            .map(|relation| relation.get().clone())
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!(
                        "no such table {} partition {}",
                        relation_id.table_id, relation_id.partition_id
                    )
                )
            })
    }

    async fn broadcast_drop_table_async(&self, oid: OID) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            self.apply_drop_table_local_async(oid).await;
            return Ok(());
        }
        for storage in peers {
            storage.apply_drop_table_local_async(oid).await;
        }
        Ok(())
    }
}
