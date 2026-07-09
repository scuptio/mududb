use mudu_sys::sync::SMutex;
use mudu_sys::time::system_time_now;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::sync::async_::AMutex;
use tracing::trace;

use crate::contract::meta_mgr::MetaMgr;
use crate::contract::partition_rule::PartitionRuleDesc;
use crate::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::meta::partition_binding_catalog::{
    load_partition_bindings_from_catalog, open_partition_binding_catalog,
    write_partition_binding_to_catalog,
};
use crate::meta::partition_placement_catalog::{
    load_partition_placements_from_catalog, open_partition_placement_catalog,
    write_partition_placement_to_catalog,
};
use crate::meta::partition_rule_catalog::{
    load_partition_rules_from_catalog, open_partition_rule_catalog, write_partition_rule_to_catalog,
};
use crate::meta::schema_catalog::{
    delete_schema_from_catalog, load_schemas_from_catalog, open_schema_catalog,
    write_schema_to_catalog,
};
use crate::storage::relation::relation::Relation;

type MetaMgrRegistry = HashMap<String, Vec<Weak<MetaMgrImpl>>>;
type DdlLockRegistry = HashMap<String, Weak<AMutex<()>>>;

fn registry() -> &'static SMutex<MetaMgrRegistry> {
    static REGISTRY: OnceLock<SMutex<MetaMgrRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(HashMap::new()))
}

fn ddl_lock_registry() -> &'static SMutex<DdlLockRegistry> {
    static DDL_LOCKS: OnceLock<SMutex<DdlLockRegistry>> = OnceLock::new();
    DDL_LOCKS.get_or_init(|| SMutex::new(HashMap::new()))
}

fn ddl_lock_for(path: &str) -> RS<Arc<AMutex<()>>> {
    let mut guard = ddl_lock_registry().lock()?;
    if let Some(existing) = guard.get(path).and_then(Weak::upgrade) {
        return Ok(existing);
    }
    let created = Arc::new(AMutex::new(()));
    let _ = guard.insert(path.to_string(), Arc::downgrade(&created));
    Ok(created)
}

#[derive(Clone)]
struct CatalogRelation {
    schema_catalog: Arc<Relation>,
    partition_rule_catalog: Arc<Relation>,
    partition_binding_catalog: Arc<Relation>,
    partition_placement_catalog: Arc<Relation>,
}
pub struct MetaMgrImpl {
    path: String,
    ddl_lock: Arc<AMutex<()>>,
    catalog: SMutex<Option<CatalogRelation>>,
    next_catalog_xid: AtomicU64,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    id2table: scc::HashMap<OID, TableInfo>,
    name2id: scc::HashMap<String, OID>,
    table: scc::HashMap<String, TableInfo>,
    rule_by_id: scc::HashMap<OID, PartitionRuleDesc>,
    rule_name2id: scc::HashMap<String, OID>,
    binding_by_table_id: scc::HashMap<OID, TablePartitionBinding>,
    placement_by_partition_id: scc::HashMap<OID, OID>,
}

impl MetaMgrImpl {
    fn catalog_relation(&self) -> RS<CatalogRelation> {
        self.catalog
            .lock()?
            .as_ref()
            .cloned()
            .ok_or_else(|| mudu_error!(ER::Internal, "meta manager is not initialized"))
    }

    pub async fn initialize_inner(&self) -> RS<()> {
        let path = PathBuf::from(&self.path);
        if !mudu_sys::fs::sync::path_exists(&path) {
            mudu_sys::fs::sync::create_dir_all(&path)?;
        }

        let schema_catalog = open_schema_catalog(&self.path, self.async_runtime.clone()).await?;
        let partition_rule_catalog =
            open_partition_rule_catalog(&self.path, self.async_runtime.clone()).await?;
        let partition_binding_catalog =
            open_partition_binding_catalog(&self.path, self.async_runtime.clone()).await?;
        let partition_placement_catalog =
            open_partition_placement_catalog(&self.path, self.async_runtime.clone()).await?;
        for schema in load_schemas_from_catalog(&schema_catalog).await? {
            self.apply_create_table_local(&schema)?;
        }
        for rule in load_partition_rules_from_catalog(&partition_rule_catalog).await? {
            self.apply_create_partition_rule_local(&rule);
        }
        for binding in load_partition_bindings_from_catalog(&partition_binding_catalog).await? {
            self.apply_bind_table_partition_local(&binding);
        }
        for placement in
            load_partition_placements_from_catalog(&partition_placement_catalog).await?
        {
            self.apply_partition_placement_local(&placement);
        }
        let catalog = CatalogRelation {
            schema_catalog: Arc::new(schema_catalog),
            partition_rule_catalog: Arc::new(partition_rule_catalog),
            partition_placement_catalog: Arc::new(partition_placement_catalog),
            partition_binding_catalog: Arc::new(partition_binding_catalog),
        };
        let mut guard = self.catalog.lock()?;
        *guard = Some(catalog);
        Ok(())
    }

    pub async fn new<P: AsRef<Path>>(path: P) -> RS<Self> {
        Self::new_with_async_runtime(path, None).await
    }

    pub async fn new_with_async_runtime<P: AsRef<Path>>(
        path: P,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    ) -> RS<Self> {
        let path = PathBuf::from(path.as_ref());
        let path_string = path.to_string_lossy().to_string();
        let ddl_lock = ddl_lock_for(&path_string)?;
        let this = Self {
            path: path.to_string_lossy().to_string(),
            ddl_lock,
            catalog: SMutex::new(None),
            next_catalog_xid: AtomicU64::new(now_catalog_xid()),
            async_runtime,
            id2table: Default::default(),
            name2id: Default::default(),
            table: Default::default(),
            rule_by_id: Default::default(),
            rule_name2id: Default::default(),
            binding_by_table_id: Default::default(),
            placement_by_partition_id: Default::default(),
        };
        // this.initialize_inner().await?;
        Ok(this)
    }

    pub fn register_global(self: &Arc<Self>) -> RS<()> {
        let mut guard = registry().lock()?;
        guard
            .entry(self.path.clone())
            .or_default()
            .push(Arc::downgrade(self));
        Ok(())
    }

    pub fn lookup_table_info_by_id(&self, oid: OID) -> Option<TableInfo> {
        let opt = self.id2table.get_sync(&oid);
        opt.map(|entry| entry.get().clone())
    }

    pub fn lookup_table_by_name(&self, name: &str) -> RS<Option<Arc<TableDesc>>> {
        let opt = self.table.get_sync(name);
        let table_desc = match opt {
            None => return Ok(None),
            Some(table) => table.get().table_desc()?,
        };
        Ok(Some(table_desc))
    }

    pub fn list_schemas_inner(&self) -> RS<Vec<SchemaTable>> {
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

    pub fn lookup_partition_rule_by_id(&self, oid: OID) -> Option<PartitionRuleDesc> {
        self.rule_by_id
            .get_sync(&oid)
            .map(|entry| entry.get().clone())
    }

    pub fn lookup_partition_rule_by_name(&self, name: &str) -> Option<PartitionRuleDesc> {
        let rule_id = self.rule_name2id.get_sync(name).map(|entry| *entry.get())?;
        self.lookup_partition_rule_by_id(rule_id)
    }

    pub fn list_partition_rules_inner(&self) -> Vec<PartitionRuleDesc> {
        let mut rules = Vec::new();
        self.rule_by_id.iter_sync(|_rule_id, rule| {
            rules.push(rule.clone());
            true
        });
        rules.sort_by_key(|rule| rule.oid);
        rules
    }

    pub fn lookup_table_partition_binding(&self, table_id: OID) -> Option<TablePartitionBinding> {
        self.binding_by_table_id
            .get_sync(&table_id)
            .map(|entry| entry.get().clone())
    }

    pub fn list_partition_placements_inner(&self) -> Vec<PartitionPlacement> {
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

    pub async fn create_table_inner(&self, schema: &SchemaTable) -> RS<()> {
        trace!(table = %schema.table_name(), oid = schema.id(), "meta_mgr create_table_inner start");
        let _ddl_guard = self.ddl_lock.lock().await;
        trace!(table = %schema.table_name(), oid = schema.id(), "meta_mgr create_table_inner acquired ddl lock");
        if self.table.contains_sync(schema.table_name()) {
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

    pub async fn drop_table_inner(&self, oid: OID) -> RS<()> {
        let _ddl_guard = self.ddl_lock.lock().await;
        let table = self
            .lookup_table_info_by_id(oid)
            .ok_or_else(|| mudu_error!(ER::EntityNotFound, format!("no such table {}", oid)))?;
        let schema_catalog = self.catalog_relation()?.schema_catalog;

        delete_schema_from_catalog(&schema_catalog, oid, self.next_catalog_xid()).await?;
        self.broadcast_drop(table.schema()?.table_name(), oid)
    }

    pub async fn create_partition_rule_inner(&self, rule: &PartitionRuleDesc) -> RS<()> {
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

    pub async fn bind_table_partition_inner(&self, binding: &TablePartitionBinding) -> RS<()> {
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

    pub async fn upsert_partition_placements_inner(
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

    fn next_catalog_xid(&self) -> u64 {
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

    fn apply_create_table_local(&self, schema: &SchemaTable) -> RS<()> {
        let table_id = schema.id();
        let table_name = schema.table_name().clone();
        let table = TableInfo::new(schema.clone())?;
        let _ = self.table.insert_sync(table_name.clone(), table.clone());
        let _ = self.id2table.insert_sync(table_id, table);
        let _ = self.name2id.insert_sync(table_name, table_id);
        Ok(())
    }

    fn apply_drop_table_local(&self, table_name: &str, oid: OID) {
        let _ = self.id2table.remove_sync(&oid);
        let _ = self.name2id.remove_sync(table_name);
        let _ = self.table.remove_sync(table_name);
    }

    fn apply_create_partition_rule_local(&self, rule: &PartitionRuleDesc) {
        let _ = self.rule_name2id.insert_sync(rule.name.clone(), rule.oid);
        let _ = self.rule_by_id.insert_sync(rule.oid, rule.clone());
    }

    fn apply_bind_table_partition_local(&self, binding: &TablePartitionBinding) {
        let _ = self
            .binding_by_table_id
            .insert_sync(binding.table_id, binding.clone());
    }

    fn apply_partition_placement_local(&self, placement: &PartitionPlacement) {
        let _ = self
            .placement_by_partition_id
            .insert_sync(placement.partition_id, placement.worker_id);
    }

    fn broadcast_create(&self, schema: &SchemaTable) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            return self.apply_create_table_local(schema);
        }
        for mgr in peers {
            mgr.apply_create_table_local(schema)?;
        }
        Ok(())
    }

    fn broadcast_drop(&self, table_name: &str, oid: OID) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            self.apply_drop_table_local(table_name, oid);
            return Ok(());
        }
        for mgr in peers {
            mgr.apply_drop_table_local(table_name, oid);
        }
        Ok(())
    }

    fn broadcast_create_partition_rule(&self, rule: &PartitionRuleDesc) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            self.apply_create_partition_rule_local(rule);
            return Ok(());
        }
        for mgr in peers {
            mgr.apply_create_partition_rule_local(rule);
        }
        Ok(())
    }

    fn broadcast_bind_table_partition(&self, binding: &TablePartitionBinding) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            self.apply_bind_table_partition_local(binding);
            return Ok(());
        }
        for mgr in peers {
            mgr.apply_bind_table_partition_local(binding);
        }
        Ok(())
    }

    fn broadcast_upsert_partition_placements(&self, placements: &[PartitionPlacement]) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            for placement in placements {
                self.apply_partition_placement_local(placement);
            }
            return Ok(());
        }
        for mgr in peers {
            for placement in placements {
                mgr.apply_partition_placement_local(placement);
            }
        }
        Ok(())
    }

    fn peer_instances(&self) -> RS<Vec<Arc<MetaMgrImpl>>> {
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

fn now_catalog_xid() -> u64 {
    system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(u64::MAX as u128) as u64
}

#[async_trait]
impl MetaMgr for MetaMgrImpl {
    async fn initialize(&self) -> RS<()> {
        self.initialize_inner().await
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

    async fn get_table_by_name(&self, name: &str) -> RS<Option<Arc<TableDesc>>> {
        self.lookup_table_by_name(name)
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
}

unsafe impl Sync for MetaMgrImpl {}

unsafe impl Send for MetaMgrImpl {}

#[cfg(test)]
mod tests {

    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use crate::contract::schema_column::SchemaColumn;
    use mudu_sys::env_var::temp_dir;
    use mudu_type::data_type_info::DataTypeInfo;
    use mudu_type::type_family::TypeFamily;
    use std::future::Future;

    use super::*;

    fn block_on<F>(fut: F) -> F::Output
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        mudu_sys::task::async_::block_on_tokio_current_thread(fut).unwrap()
    }

    fn test_schema() -> SchemaTable {
        SchemaTable::new(
            "meta_recovery_t".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
                SchemaColumn::new(
                    "v".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
            ],
            vec![0],
            vec![1],
        )
    }

    #[test]
    fn meta_mgr_recovers_schema_catalog_after_reopen() {
        block_on(async move {
            let r = _meta_mgr_recovers_schema_catalog_after_reopen().await;
            assert!(r.is_ok());
        });
    }
    async fn _meta_mgr_recovers_schema_catalog_after_reopen() -> RS<()> {
        let dir = temp_dir().join(format!("meta_mgr_catalog_{}", mudu_utils::oid::gen_oid()));
        let initial_dir = dir.clone();
        let mgr = Arc::new(MetaMgrImpl::new(initial_dir).await?);
        mgr.register_global()?;
        mgr.initialize().await?;
        let _mgr = mgr.clone();
        let schema = test_schema();
        let _schema = schema.clone();
        _mgr.create_table(&_schema).await?;
        let schema_catalog = mgr.catalog_relation()?.schema_catalog;
        assert_eq!(load_schemas_from_catalog(&schema_catalog).await?.len(), 1);
        drop(mgr);

        let reopened = MetaMgrImpl::new(dir).await?;
        reopened.initialize().await?;
        let schema_id = schema.id();
        let table = reopened.get_table_by_id(schema_id).await?;
        assert_eq!(table.name(), schema.table_name());
        Ok(())
    }

    #[test]
    fn meta_mgr_broadcasts_ddl_to_peer_instances() {
        block_on(async move {
            let r = _meta_mgr_broadcasts_ddl_to_peer_instances().await;
            assert!(r.is_ok());
        });
    }
    async fn _meta_mgr_broadcasts_ddl_to_peer_instances() -> RS<()> {
        let dir = temp_dir().join(format!("meta_mgr_peer_{}", mudu_utils::oid::gen_oid()));
        let mgr1 = Arc::new(MetaMgrImpl::new(&dir).await?);
        mgr1.register_global()?;
        mgr1.initialize().await?;
        let mgr2 = Arc::new(MetaMgrImpl::new(&dir).await?);
        mgr2.register_global()?;
        mgr2.initialize().await?;

        let schema = test_schema();
        mgr1.create_table(&schema).await?;
        let table = mgr2.get_table_by_id(schema.id()).await?;
        assert_eq!(table.name(), schema.table_name());

        mgr2.drop_table(schema.id()).await?;
        assert!(mgr1.get_table_by_id(schema.id()).await.is_err());
        Ok(())
    }
}
