//! Catalog-versioned cache of bound parameter templates ("plan cache").
//!
//! Binding a SQL statement requires catalog lookups and a full AST walk, and
//! benchmark workloads re-execute a small set of parameterized statement
//! templates thousands of times per second. The bound shape of a statement
//! depends only on the SQL text and the catalog, so this cache stores
//! [`BoundTemplate`]s (placeholders kept as ordered slots, see
//! `sql::bound_template`) keyed by SQL text, tagged with the catalog version
//! they were bound against. A lookup carries the current
//! `MetaMgr::catalog_version()`; a version mismatch is a miss and the entry
//! is overwritten after rebinding, which is how DDL invalidates cached
//! templates.
//!
//! Cached templates are classified (`PlanClass`): point reads/updates/inserts
//! execute as direct `XContract` calls (equivalent to the executor path,
//! which issues exactly these calls), everything else is filled and fed to
//! the regular planner, saving only the bind.
//!
//! The cache is sharded like `stmt_parse_cache` so concurrent workers do not
//! serialize on one lock. Entries are keyed by SQL text only (the catalog
//! version lives inside the entry) so lookups borrow the text without
//! allocating a key string.

use crate::contract::meta_mgr::MetaMgr;
use crate::mudu_conn::mudu_conn_core::{query_exec_to_rows, tuple_field_to_value};
use crate::sql::bound_stmt::BoundStmt;
use crate::sql::bound_template::{
    fill_pairs, BoundTemplate, PlanClass, PredicateTemplate, SetValueTemplate, StmtTemplate,
};
use crate::sql::plan_ctx::PlanCtx;
use crate::sql::planner::Planner;
use crate::x_engine::api::{
    DeltaAssign, OptInsert, OptRead, OptUpdate, Predicate, VecDatum, VecSelTerm, XContract,
};
use crate::x_engine::tx_mgr::TxMgr;
use mudu::common::id::AttrIndex;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::tuple::tuple_field::TupleField;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::sync::SMutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Upper bound on cached templates; beyond it new entries are simply not
/// inserted (pathological workloads with millions of distinct SQL texts fall
/// back to binding every time). Mirrors the parse cache policy.
const PLAN_CACHE_CAPACITY: usize = 4096;

/// Number of independently locked shards; see `stmt_parse_cache` for the
/// rationale of the value.
const PLAN_CACHE_SHARDS: usize = 16;

/// Per-shard capacity. A shard that fills up simply stops accepting new
/// entries; existing entries are still replaced (DDL rebinding).
const PLAN_CACHE_SHARD_CAPACITY: usize = PLAN_CACHE_CAPACITY / PLAN_CACHE_SHARDS;

/// FNV-1a over the SQL text, used only for shard selection (same reasoning
/// as the parse cache).
fn shard_index(sql: &str) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in sql.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash as usize) % PLAN_CACHE_SHARDS
}

struct PlanCacheEntry {
    /// Catalog version the template was bound against; a mismatch with the
    /// current version is a cache miss.
    catalog_version: u64,
    plan: Arc<CachedPlan>,
}

/// A classified bound template ready for repeated execution.
pub(crate) struct CachedPlan {
    template: BoundTemplate,
    class: PlanClass,
}

impl CachedPlan {
    pub(crate) fn new(template: BoundTemplate) -> Self {
        let class = template.classify();
        Self { template, class }
    }

    /// Executes the template as a query: a point read issues one
    /// `XContract::read_key` and materializes the row like
    /// `query_exec_to_rows` does; anything else is filled and run through
    /// the regular planner and executors.
    pub(crate) async fn run_query(
        &self,
        params: &dyn SQLParams,
        tx_mgr: Arc<dyn TxMgr>,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    ) -> RS<(Vec<TupleValue>, TupleFieldDesc)> {
        if let (PlanClass::PointRead { select }, StmtTemplate::Select(template)) =
            (&self.class, &self.template.stmt)
        {
            return self
                .run_point_read(template, select, params, tx_mgr, x_contract)
                .await;
        }
        let bound = self.template.fill(params)?;
        let BoundStmt::Query(query) = bound else {
            return Err(mudu_error!(
                ErrorCode::InvalidType,
                "statement is not a query"
            ));
        };
        let planner = Planner::new(PlanCtx {
            tx_mgr,
            meta_mgr,
            x_contract,
            async_runtime,
        });
        let exec = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::SqlPlan,
            );
            planner.plan_query(query).await?
        };
        let _stage =
            crate::server::stage_stats::StageGuard::new(crate::server::stage_stats::Stage::SqlRun);
        query_exec_to_rows(exec).await
    }

    /// Executes the template as a command: point updates/inserts issue direct
    /// `XContract::update`/`insert` calls (with the same argument split and
    /// pre-checks as the command executors); anything else is filled and run
    /// through the regular planner and command executors.
    pub(crate) async fn run_execute(
        &self,
        params: &dyn SQLParams,
        tx_mgr: Arc<dyn TxMgr>,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    ) -> RS<u64> {
        match (&self.class, &self.template.stmt) {
            (PlanClass::PointUpdate, StmtTemplate::Update(template)) => {
                self.run_point_update(template, params, tx_mgr, x_contract)
                    .await
            }
            (PlanClass::PointInsert, StmtTemplate::Insert(template)) => {
                self.run_point_insert(template, params, tx_mgr, x_contract)
                    .await
            }
            (_, StmtTemplate::Select(_)) => Err(mudu_error!(
                ErrorCode::InvalidType,
                "statement is not a command"
            )),
            _ => {
                let bound = self.template.fill(params)?;
                let BoundStmt::Command(command) = bound else {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "statement is not a command"
                    ));
                };
                let planner = Planner::new(PlanCtx {
                    tx_mgr,
                    meta_mgr,
                    x_contract,
                    async_runtime,
                });
                let cmd = {
                    let _stage = crate::server::stage_stats::StageGuard::new(
                        crate::server::stage_stats::Stage::SqlPlan,
                    );
                    planner.plan_command(command).await?
                };
                {
                    let _stage = crate::server::stage_stats::StageGuard::new(
                        crate::server::stage_stats::Stage::SqlPrepare,
                    );
                    cmd.prepare().await?;
                }
                {
                    let _stage = crate::server::stage_stats::StageGuard::new(
                        crate::server::stage_stats::Stage::SqlRun,
                    );
                    cmd.run().await?;
                }
                cmd.affected_rows().await
            }
        }
    }

    /// Point read: one `read_key` with the filled key (equivalent to
    /// `IndexAccessKey`, which issues exactly this call) plus the SQL result
    /// materialization.
    async fn run_point_read(
        &self,
        template: &crate::sql::bound_template::SelectTemplate,
        select: &[AttrIndex],
        params: &dyn SQLParams,
        tx_mgr: Arc<dyn TxMgr>,
        x_contract: Arc<dyn XContract>,
    ) -> RS<(Vec<TupleValue>, TupleFieldDesc)> {
        let _stage =
            crate::server::stage_stats::StageGuard::new(crate::server::stage_stats::Stage::SqlRun);
        let PredicateTemplate::KeyEq { key } = &template.predicate else {
            return Err(mudu_error!(
                ErrorCode::Internal,
                "point read template without key-equality predicate"
            ));
        };
        let key = fill_pairs(key, &self.template.slots, params)?;
        let row = x_contract
            .read_key(
                tx_mgr,
                template.table_id,
                &VecDatum::new(key),
                &VecSelTerm::new(select.to_vec()),
                &OptRead::default(),
            )
            .await?;
        let mut rows = Vec::new();
        if let Some(fields) = row {
            let value = {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::ResultDecode,
                );
                tuple_field_to_value(TupleField::new_nullable(fields), &template.tuple_desc)?
            };
            rows.push(value);
        }
        Ok((rows, template.tuple_desc.clone()))
    }

    /// Point update: fills the key and splits absolute/delta assignments
    /// exactly like `Planner::plan_update`, then issues one
    /// `XContract::update`. The key/value emptiness checks mirror
    /// `UpdateKeyValue::prepare`.
    async fn run_point_update(
        &self,
        template: &crate::sql::bound_template::UpdateTemplate,
        params: &dyn SQLParams,
        tx_mgr: Arc<dyn TxMgr>,
        x_contract: Arc<dyn XContract>,
    ) -> RS<u64> {
        let _stage =
            crate::server::stage_stats::StageGuard::new(crate::server::stage_stats::Stage::SqlRun);
        let slots = &self.template.slots;
        let key = fill_pairs(&template.key, slots, params)?;
        if key.is_empty() {
            return Err(mudu_error!(
                ErrorCode::EntityNotFound,
                "update key is empty"
            ));
        }
        let mut absolute = Vec::new();
        let mut delta_assignments = Vec::new();
        for (attr, set_value) in &template.value {
            match set_value {
                SetValueTemplate::Absolute(datum) => {
                    absolute.push((*attr, datum.fill_some(slots, params)?))
                }
                SetValueTemplate::Delta { op, operand } => delta_assignments.push(DeltaAssign {
                    attr: *attr,
                    op: *op,
                    literal: operand.fill_some(slots, params)?,
                }),
            }
        }
        if absolute.is_empty() && delta_assignments.is_empty() {
            return Err(mudu_error!(
                ErrorCode::EntityNotFound,
                "update value is empty"
            ));
        }
        let updated = x_contract
            .update(
                tx_mgr,
                template.table_id,
                &VecDatum::new(key),
                &Predicate::CNF(Vec::new()),
                &VecDatum::new(absolute),
                &OptUpdate { delta_assignments },
            )
            .await?;
        Ok(updated as u64)
    }

    /// Point insert: fills every row and issues one `XContract::insert` per
    /// row. All rows are validated before the first insert, mirroring
    /// `InsertKeyValue::prepare`.
    async fn run_point_insert(
        &self,
        template: &crate::sql::bound_template::InsertTemplate,
        params: &dyn SQLParams,
        tx_mgr: Arc<dyn TxMgr>,
        x_contract: Arc<dyn XContract>,
    ) -> RS<u64> {
        let _stage =
            crate::server::stage_stats::StageGuard::new(crate::server::stage_stats::Stage::SqlRun);
        let slots = &self.template.slots;
        let mut rows = Vec::with_capacity(template.rows.len());
        for row in &template.rows {
            rows.push((
                VecDatum::new(fill_pairs(&row.key, slots, params)?),
                VecDatum::new(fill_pairs(&row.value, slots, params)?),
            ));
        }
        for (key, _) in &rows {
            if key.data().is_empty() {
                return Err(mudu_error!(ErrorCode::EntityNotFound, "key is empty"));
            }
        }
        let mut affected_rows = 0;
        for (key, value) in &rows {
            x_contract
                .insert(
                    tx_mgr.clone(),
                    template.table_id,
                    key,
                    value,
                    &OptInsert::default(),
                )
                .await?;
            affected_rows += 1;
        }
        Ok(affected_rows)
    }
}

/// Sharded plan cache. One instance per worker runtime; entries are shared
/// behind `Arc` so a hit costs a shard lock plus an atomic refcount bump.
pub(crate) struct PlanCache {
    shards: [SMutex<HashMap<String, PlanCacheEntry>>; PLAN_CACHE_SHARDS],
    hits: AtomicU64,
    misses: AtomicU64,
}

impl PlanCache {
    pub(crate) fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| SMutex::new(HashMap::new())),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Returns the cached plan for `sql` when an entry exists and was bound
    /// against `catalog_version`; a version mismatch is a miss (the stale
    /// entry is left in place and overwritten by the next `insert`).
    pub(crate) fn get(&self, sql: &str, catalog_version: u64) -> RS<Option<Arc<CachedPlan>>> {
        let guard = self.shards[shard_index(sql)]
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "plan cache lock poisoned"))?;
        match guard.get(sql) {
            Some(entry) if entry.catalog_version == catalog_version => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Ok(Some(entry.plan.clone()))
            }
            _ => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
        }
    }

    /// Caches `template` under `sql` and `catalog_version`, replacing any
    /// existing entry (DDL rebinding overwrites the stale template), and
    /// returns the shared plan for immediate execution.
    pub(crate) fn insert(
        &self,
        sql: String,
        catalog_version: u64,
        template: BoundTemplate,
    ) -> RS<Arc<CachedPlan>> {
        let plan = Arc::new(CachedPlan::new(template));
        let mut guard = self.shards[shard_index(&sql)]
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "plan cache lock poisoned"))?;
        if guard.contains_key(&sql) {
            let entry = guard
                .get_mut(&sql)
                .ok_or_else(|| mudu_error!(ErrorCode::Internal, "plan cache entry vanished"))?;
            entry.catalog_version = catalog_version;
            entry.plan = plan.clone();
        } else if guard.len() < PLAN_CACHE_SHARD_CAPACITY {
            guard.insert(
                sql,
                PlanCacheEntry {
                    catalog_version,
                    plan: plan.clone(),
                },
            );
        }
        Ok(plan)
    }

    /// Cache hit/miss counters (relaxed; diagnostics and tests only).
    #[cfg(test)]
    pub(crate) fn stats(&self) -> (u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
        )
    }
}

impl std::fmt::Debug for PlanCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanCache")
            .field("hits", &self.hits.load(Ordering::Relaxed))
            .field("misses", &self.misses.load(Ordering::Relaxed))
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::sql::bound_template::DeleteTemplate;

    fn dummy_template(table_id: u128) -> BoundTemplate {
        BoundTemplate::new(
            StmtTemplate::Delete(DeleteTemplate {
                table_id,
                key: Vec::new(),
            }),
            Vec::new(),
        )
    }

    #[test]
    fn empty_cache_misses() {
        let cache = PlanCache::new();
        assert!(cache.get("select 1", 0).unwrap().is_none());
        let (hits, misses) = cache.stats();
        assert_eq!((hits, misses), (0, 1));
    }

    #[test]
    fn same_version_hits_and_shares_one_plan() {
        let cache = PlanCache::new();
        let inserted = cache
            .insert(
                "delete from t where id = ?".to_string(),
                7,
                dummy_template(1),
            )
            .unwrap();
        let hit = cache.get("delete from t where id = ?", 7).unwrap().unwrap();
        assert!(Arc::ptr_eq(&inserted, &hit));
        let (hits, _) = cache.stats();
        assert_eq!(hits, 1);
    }

    #[test]
    fn version_mismatch_is_miss_and_rebind_overwrites() {
        let cache = PlanCache::new();
        let sql = "delete from t where id = ?".to_string();
        cache.insert(sql.clone(), 1, dummy_template(1)).unwrap();
        // A DDL bumped the catalog version: the stale entry misses.
        assert!(cache.get(&sql, 2).unwrap().is_none());
        // Rebinding overwrites the entry under the new version.
        let rebound = cache.insert(sql.clone(), 2, dummy_template(2)).unwrap();
        let hit = cache.get(&sql, 2).unwrap().unwrap();
        assert!(Arc::ptr_eq(&rebound, &hit));
        // The old version still misses.
        assert!(cache.get(&sql, 1).unwrap().is_none());
    }

    #[test]
    fn distinct_sql_texts_do_not_collide() {
        let cache = PlanCache::new();
        cache.insert("a".to_string(), 0, dummy_template(1)).unwrap();
        assert!(cache.get("b", 0).unwrap().is_none());
        assert!(cache.get("a", 0).unwrap().is_some());
    }
}
