mod apply;
mod ddl;
mod lookup;
#[cfg(test)]
mod tests;
mod trait_impl;

use mudu_sys::sync::async_::AMutex;
use mudu_sys::sync::SMutex;
use mudu_sys::time::system_time_now;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock, Weak};
use std::time::UNIX_EPOCH;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;

use crate::fs_object::{fs_object_schema, FS_OBJECT_TABLE_NAME};
use crate::fs_type_catalog::{load_fs_types_from_catalog, open_fs_type_catalog};
use crate::partition_binding_catalog::{
    load_partition_bindings_from_catalog, open_partition_binding_catalog,
};
use crate::partition_placement_catalog::{
    load_partition_placements_from_catalog, open_partition_placement_catalog,
};
use crate::partition_rule_catalog::{
    load_partition_rules_from_catalog, open_partition_rule_catalog,
};
use crate::schema_catalog::{load_schemas_from_catalog, open_schema_catalog};
use ::catalog::contract::app_schema::DEFAULT_SCHEMA;
use ::catalog::contract::fs_type::FsTypeDesc;
use ::catalog::contract::partition_rule::PartitionRuleDesc;
use ::catalog::contract::partition_rule_binding::TablePartitionBinding;
use ::catalog::contract::table_info::TableInfo;
use ::storage::relation::relation::Relation;

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
    fs_type: Arc<Relation>,
}

impl CatalogRelation {
    /// Writes back dirty data pages of every catalog relation; see
    /// [`Relation::flush_dirty_pages`]. The meta catalog relations live
    /// outside `WorkerStorage::relation_store`, so the worker flush driver
    /// reaches them through this method. Yields cooperatively between
    /// catalog relations so the sweep cannot monopolize the worker event
    /// loop.
    async fn flush_dirty_pages(&self) -> RS<()> {
        self.schema_catalog.flush_dirty_pages().await?;
        ::mudu_utils::yield_now::cooperative_yield_now().await;
        self.partition_rule_catalog.flush_dirty_pages().await?;
        ::mudu_utils::yield_now::cooperative_yield_now().await;
        self.partition_binding_catalog.flush_dirty_pages().await?;
        ::mudu_utils::yield_now::cooperative_yield_now().await;
        self.partition_placement_catalog.flush_dirty_pages().await?;
        ::mudu_utils::yield_now::cooperative_yield_now().await;
        self.fs_type.flush_dirty_pages().await?;
        Ok(())
    }
}
/// The in-memory meta manager: caches of table schemas, partition
/// rules/bindings/placements, and filesystem types over the on-disk system
/// catalogs under `path`.
pub struct MetaMgrImpl {
    path: String,
    ddl_lock: Arc<AMutex<()>>,
    catalog: SMutex<Option<CatalogRelation>>,
    next_catalog_xid: AtomicU64,
    /// Monotonic schema version, bumped once per applied DDL change. Used by
    /// plan caches to detect catalog invalidation. Distinct from
    /// `next_catalog_xid`, which is an MVCC timestamp for catalog rows.
    catalog_version: AtomicU64,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    id2table: scc::HashMap<OID, TableInfo>,
    // Table identity is the OID; the name maps are keyed by (schema, name) so
    // each application owns a namespace, with `id2table` staying global.
    name2id: scc::HashMap<(String, String), OID>,
    table: scc::HashMap<(String, String), TableInfo>,
    rule_by_id: scc::HashMap<OID, PartitionRuleDesc>,
    rule_name2id: scc::HashMap<String, OID>,
    binding_by_table_id: scc::HashMap<OID, TablePartitionBinding>,
    placement_by_partition_id: scc::HashMap<OID, OID>,
    fs_type_by_name: scc::HashMap<String, FsTypeDesc>,
    fs_type_by_id: scc::HashMap<u64, FsTypeDesc>,
    next_fs_id: AtomicU64,
}

impl MetaMgrImpl {
    fn catalog_relation(&self) -> RS<CatalogRelation> {
        self.catalog
            .lock()?
            .as_ref()
            .cloned()
            .ok_or_else(|| mudu_error!(ER::Internal, "meta manager is not initialized"))
    }

    pub(crate) async fn initialize_inner(&self) -> RS<()> {
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
        let fs_type_catalog = open_fs_type_catalog(&self.path, self.async_runtime.clone()).await?;
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
        for desc in load_fs_types_from_catalog(&fs_type_catalog).await? {
            self.apply_create_fs_type_local(&desc);
        }
        // Register the built-in `_fs_object` system table so it resolves like
        // any user table and its relations get bootstrapped from the schema
        // list. It is not persisted in the schema catalog; a user table with
        // the same name fails as a duplicate. It lives in the default schema,
        // so applications reach it through the search-path fallback.
        if !self
            .table
            .contains_sync(&(DEFAULT_SCHEMA.to_string(), FS_OBJECT_TABLE_NAME.to_string()))
        {
            self.apply_create_table_local(&fs_object_schema())?;
        }
        let catalog = CatalogRelation {
            schema_catalog: Arc::new(schema_catalog),
            partition_rule_catalog: Arc::new(partition_rule_catalog),
            partition_placement_catalog: Arc::new(partition_placement_catalog),
            partition_binding_catalog: Arc::new(partition_binding_catalog),
            fs_type: Arc::new(fs_type_catalog),
        };
        let mut guard = self.catalog.lock()?;
        *guard = Some(catalog);
        Ok(())
    }

    /// Creates a meta manager rooted at `path` (without an async I/O
    /// runtime).
    pub async fn new<P: AsRef<Path>>(path: P) -> RS<Self> {
        Self::new_with_async_runtime(path, None).await
    }

    pub(crate) async fn new_with_async_runtime<P: AsRef<Path>>(
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
            catalog_version: AtomicU64::new(0),
            async_runtime,
            id2table: Default::default(),
            name2id: Default::default(),
            table: Default::default(),
            rule_by_id: Default::default(),
            rule_name2id: Default::default(),
            binding_by_table_id: Default::default(),
            placement_by_partition_id: Default::default(),
            fs_type_by_name: Default::default(),
            fs_type_by_id: Default::default(),
            next_fs_id: AtomicU64::new(1),
        };
        // this.initialize_inner().await?;
        Ok(this)
    }

    pub(crate) fn register_global(self: &Arc<Self>) -> RS<()> {
        let mut guard = registry().lock()?;
        guard
            .entry(self.path.clone())
            .or_default()
            .push(Arc::downgrade(self));
        Ok(())
    }
}

fn now_catalog_xid() -> u64 {
    system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(u64::MAX as u128) as u64
}

unsafe impl Sync for MetaMgrImpl {}

unsafe impl Send for MetaMgrImpl {}
