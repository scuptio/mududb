use mudu_sys::sync::SMutex;
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::partition_rule::PartitionRuleDesc;
use ::catalog::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;

/// In-memory [`MetaMgr`] fixture for tests and fuzz targets.
pub struct TestMetaMgr {
    schemas: SMutex<HashMap<OID, SchemaTable>>,
    tables: SMutex<HashMap<OID, Arc<TableDesc>>>,
    rules: SMutex<HashMap<OID, PartitionRuleDesc>>,
    bindings: SMutex<HashMap<OID, TablePartitionBinding>>,
    placements: SMutex<HashMap<OID, OID>>,
}

impl TestMetaMgr {
    /// Creates an empty in-memory catalog.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            schemas: SMutex::new(HashMap::new()),
            tables: SMutex::new(HashMap::new()),
            rules: SMutex::new(HashMap::new()),
            bindings: SMutex::new(HashMap::new()),
            placements: SMutex::new(HashMap::new()),
        }
    }

    /// Seeds a partition rule (bypasses the `CREATE PARTITION RULE` command).
    pub fn add_partition_rule(&self, rule: PartitionRuleDesc) {
        self.rules.lock().unwrap().insert(rule.oid, rule);
    }

    /// Seeds a partition placement (bypasses the `CREATE PARTITION PLACEMENT`
    /// command).
    pub fn place_partition(&self, partition_id: OID, worker_id: OID) {
        self.placements
            .lock()
            .unwrap()
            .insert(partition_id, worker_id);
    }
}

#[async_trait]
impl MetaMgr for TestMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }

    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
        self.tables
            .lock()
            .unwrap()
            .get(&oid)
            .cloned()
            .ok_or_else(|| mudu_error!(ErrorCode::EntityNotFound, format!("no such table {}", oid)))
    }

    async fn get_table_by_name(&self, _schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(self
            .tables
            .lock()
            .unwrap()
            .values()
            .find(|table| table.name() == name)
            .cloned())
    }

    async fn create_table(&self, schema: &SchemaTable) -> RS<()> {
        let table = TableInfo::new(schema.clone())?.table_desc()?;
        self.schemas
            .lock()
            .unwrap()
            .insert(schema.id(), schema.clone());
        self.tables.lock().unwrap().insert(schema.id(), table);
        Ok(())
    }

    async fn drop_table(&self, table_id: OID) -> RS<()> {
        self.schemas.lock().unwrap().remove(&table_id);
        self.tables.lock().unwrap().remove(&table_id);
        Ok(())
    }

    async fn create_queue(&self, schema: &SchemaTable) -> RS<()> {
        if !schema.is_queue() {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "schema does not carry the queue kind"
            ));
        }
        self.create_table(schema).await
    }

    async fn drop_queue(&self, queue_id: OID) -> RS<()> {
        let is_queue = self
            .schemas
            .lock()
            .unwrap()
            .get(&queue_id)
            .map(|schema| schema.is_queue())
            .unwrap_or(false);
        if !is_queue {
            return Err(mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such queue {}", queue_id)
            ));
        }
        self.drop_table(queue_id).await
    }

    async fn get_queue(&self, schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        match self.get_table_by_name(schema, name).await? {
            Some(desc) if desc.is_queue() => Ok(Some(desc)),
            _ => Ok(None),
        }
    }

    async fn list_queues(&self) -> RS<Vec<SchemaTable>> {
        Ok(self
            .schemas
            .lock()
            .unwrap()
            .values()
            .filter(|schema| schema.is_queue())
            .cloned()
            .collect())
    }

    async fn list_schemas(&self) -> RS<Vec<SchemaTable>> {
        Ok(self.schemas.lock().unwrap().values().cloned().collect())
    }

    async fn create_partition_rule(&self, rule: &PartitionRuleDesc) -> RS<()> {
        self.rules.lock().unwrap().insert(rule.oid, rule.clone());
        Ok(())
    }

    async fn get_partition_rule_by_id(&self, oid: OID) -> RS<PartitionRuleDesc> {
        self.rules
            .lock()
            .unwrap()
            .get(&oid)
            .cloned()
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("no such partition rule {}", oid)
                )
            })
    }

    async fn get_partition_rule_by_name(&self, name: &str) -> RS<Option<PartitionRuleDesc>> {
        Ok(self
            .rules
            .lock()
            .unwrap()
            .values()
            .find(|rule| rule.name == name)
            .cloned())
    }

    async fn list_partition_rules(&self) -> RS<Vec<PartitionRuleDesc>> {
        Ok(self.rules.lock().unwrap().values().cloned().collect())
    }

    async fn bind_table_partition(&self, binding: &TablePartitionBinding) -> RS<()> {
        self.bindings
            .lock()
            .unwrap()
            .insert(binding.table_id, binding.clone());
        Ok(())
    }

    async fn get_table_partition_binding(
        &self,
        table_id: OID,
    ) -> RS<Option<TablePartitionBinding>> {
        Ok(self.bindings.lock().unwrap().get(&table_id).cloned())
    }

    async fn list_partition_bindings(&self) -> RS<Vec<TablePartitionBinding>> {
        Ok(self.bindings.lock().unwrap().values().cloned().collect())
    }

    async fn upsert_partition_placements(&self, placements: &[PartitionPlacement]) -> RS<()> {
        let mut guard = self.placements.lock().unwrap();
        for placement in placements {
            guard.insert(placement.partition_id, placement.worker_id);
        }
        Ok(())
    }

    async fn get_partition_worker(&self, partition_id: OID) -> RS<Option<OID>> {
        Ok(self.placements.lock().unwrap().get(&partition_id).copied())
    }

    async fn list_partition_placements(&self) -> RS<Vec<PartitionPlacement>> {
        Ok(self
            .placements
            .lock()
            .unwrap()
            .iter()
            .map(|(partition_id, worker_id)| PartitionPlacement {
                partition_id: *partition_id,
                worker_id: *worker_id,
            })
            .collect())
    }
}
