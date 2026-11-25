use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::xengine::x_param::PCreateCache;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_sys::sync::async_::AMutex;
use std::sync::Arc;
use tracing::trace;

/// Executor of a create-cache command.
///
/// A cache is a volatile, non-replicated catalog object: unlike a table or
/// queue, it creates no storage and never travels the replicated MetaGroup
/// log. The executor only registers the cache schema (mirroring the `LIKE`
/// table) in the catalog; its contents are always reconstructed from the
/// authoritative table.
pub struct CreateCache {
    inner: AMutex<_InnerCreateCache>,
}

struct _InnerCreateCache {
    param: PCreateCache,
    meta_mgr: Arc<dyn MetaMgr>,
}

impl CreateCache {
    /// Creates an executor for the create-cache command `param`, registering
    /// the cache schema through `meta_mgr`.
    pub fn new(param: PCreateCache, meta_mgr: Arc<dyn MetaMgr>) -> Self {
        Self {
            inner: AMutex::new(_InnerCreateCache::new(param, meta_mgr)),
        }
    }
}

#[async_trait]
impl CmdExec for CreateCache {
    async fn prepare(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let inner = self.inner.lock().await;
        inner.prepare().await
    }

    async fn run(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let mut inner = self.inner.lock().await;
        inner.run().await
    }

    async fn affected_rows(&self) -> RS<u64> {
        mudu_utils::scoped_task_trace!();
        Ok(0)
    }
}

impl _InnerCreateCache {
    fn new(param: PCreateCache, meta_mgr: Arc<dyn MetaMgr>) -> Self {
        Self { param, meta_mgr }
    }

    async fn prepare(&self) -> RS<()> {
        let cache_name = self.param.schema.class_name().clone();
        // Caches share the (schema, name) namespace with tables and queues.
        if self
            .meta_mgr
            .get_table_exact(self.param.schema.schema(), &cache_name)
            .await?
            .is_some()
        {
            return Err(mudu_error!(
                ER::ObjectAlreadyExists,
                format!("cache {} already exists", cache_name)
            ));
        }
        Ok(())
    }

    async fn run(&mut self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        trace!(
            cache = %self.param.schema.class_name(),
            ttl_secs = ?self.param.ttl_secs,
            "create_cache command run start"
        );
        // No storage creation and no replicated MetaGroup apply: a cache only
        // registers its schema locally (the authoritative table is untouched).
        // The DDL TTL becomes the cache's default entry TTL, persisted with
        // the schema so it survives a restart and applies to every put that
        // carries no per-call TTL.
        let mut schema = self.param.schema.clone();
        schema.set_ttl_secs(self.param.ttl_secs);
        self.meta_mgr.create_cache(&schema).await?;
        trace!(cache = %self.param.schema.class_name(), "create_cache command run done");
        Ok(())
    }
}
