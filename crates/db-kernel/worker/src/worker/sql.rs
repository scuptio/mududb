use super::*;
use ::catalog::contract::app_schema::current_schema;
use ::conn::mudu_conn_core::MuduConnCore;
use ::conn::mudu_result_set_async::MuduResultSetAsync;
use ::conn::plan_cache::CachedPlan;
use ::conn::worker_local::WorkerExecute;
use ::sql::binder::Binder;
use ::xengine::tx_mgr::TxMgr;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_utils::task_trace;

/// Transaction-control statements recognized on the SQL text before parsing,
/// so that clients of the query/execute protocol can drive multi-statement
/// transactions explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TxControlStmt {
    Begin,
    Commit,
    Rollback,
}

/// Recognize `BEGIN [TRANSACTION|WORK]` / `START TRANSACTION`,
/// `COMMIT [TRANSACTION|WORK]` and `ROLLBACK [TRANSACTION|WORK]`
/// (case-insensitive, optional trailing semicolons). Returns `None` for any
/// other statement, which then flows through the normal SQL pipeline.
pub(crate) fn parse_tx_control_stmt(sql: &str) -> Option<TxControlStmt> {
    let normalized = sql.trim().trim_end_matches(';').trim();
    const VARIANTS: &[(&str, TxControlStmt)] = &[
        ("BEGIN", TxControlStmt::Begin),
        ("BEGIN TRANSACTION", TxControlStmt::Begin),
        ("BEGIN WORK", TxControlStmt::Begin),
        ("START TRANSACTION", TxControlStmt::Begin),
        ("COMMIT", TxControlStmt::Commit),
        ("COMMIT TRANSACTION", TxControlStmt::Commit),
        ("COMMIT WORK", TxControlStmt::Commit),
        ("ROLLBACK", TxControlStmt::Rollback),
        ("ROLLBACK TRANSACTION", TxControlStmt::Rollback),
        ("ROLLBACK WORK", TxControlStmt::Rollback),
    ];
    VARIANTS
        .iter()
        .find(|(text, _)| normalized.eq_ignore_ascii_case(text))
        .map(|(_, control)| *control)
}

impl WorkerRuntime {
    /// Route a transaction-control statement (BEGIN/COMMIT/ROLLBACK) received
    /// as SQL text to the session transaction lifecycle. BEGIN on a session
    /// that already has an active transaction fails with
    /// `ObjectAlreadyExists` (no nesting); COMMIT goes through the routed
    /// commit so staged remote writes are handed off to their owning worker.
    async fn execute_tx_control_stmt(&self, session_id: OID, control: TxControlStmt) -> RS<()> {
        let instruction = match control {
            TxControlStmt::Begin => WorkerExecute::BeginTx,
            TxControlStmt::Commit => WorkerExecute::CommitTx,
            TxControlStmt::Rollback => WorkerExecute::RollbackTx,
        };
        self.execute_tx_async(session_id, instruction).await
    }

    fn sql_core(&self, oid: OID) -> RS<Arc<MuduConnCore>> {
        if oid == 0 {
            return Ok(Arc::new(MuduConnCore::new(
                self.meta_mgr(),
                self.contract.async_runtime(),
                false,
            )?));
        }
        Ok(self.session_context(oid)?.mudu_conn_core())
    }

    pub(crate) fn sql_tx_mgr(&self, oid: OID) -> RS<Option<Arc<dyn TxMgr>>> {
        if oid == 0 {
            return Ok(None);
        }
        self.session_manager.with_session_tx(oid, Ok)
    }

    /// Whether the statement is an autocommit-mode `RECEIVE ... WAIT`
    /// (text pre-filter on the RECEIVE keyword so only queue statements pay
    /// the confirming parse; RECEIVE is never plan-cached, so the parse is
    /// not duplicated against the cache).
    fn classify_receive_wait(&self, core: &MuduConnCore, sql: &dyn SQLStmt) -> RS<bool> {
        let text = sql.to_sql_string();
        let trimmed = text.trim_start();
        if !trimmed
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("receive"))
        {
            return Ok(false);
        }
        let stmt = core.parse_one_text(&text)?;
        Ok(::sql::binder::is_receive_wait_stmt(&stmt))
    }

    /// Rejects `CREATE CACHE` in cluster deployments: a cache registers
    /// node-local volatile state that never travels the replicated control
    /// plane, so on a cluster node the statement would silently diverge
    /// between members. The caller classifies the statement (the worker crate
    /// does not name the parser's statement type).
    fn reject_cluster_create_cache(&self, is_create_cache: bool) -> RS<()> {
        if self.cluster_router.is_some() && is_create_cache {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "CREATE CACHE is not supported in cluster deployments: caches are volatile, node-local objects"
            ));
        }
        Ok(())
    }

    /// Read-your-writes gate of a fresh session transaction (epoch design
    /// doc §5.2): wait out the commit watermark the session recorded at its
    /// last successful commit BEFORE the new transaction registers — a
    /// registered waiter would pin the epoch barrier it waits on (and an
    /// enclosing transaction's commit would deadlock against it).
    pub(crate) async fn wait_session_commit_epoch(&self, oid: OID) -> RS<()> {
        let watermark = self.session_manager.session_commit_epoch(oid)?;
        if watermark != 0 {
            self.contract.worker_wait_epoch_stable(watermark).await?;
        }
        Ok(())
    }

    /// Struct the session's commit watermark after a successful commit:
    /// the commit epoch returned by the commit path (the exact epoch the
    /// versions were stamped with — for a cross-group 2PC, the decision
    /// epoch, which may exceed this node's pre-commit view).
    pub(crate) fn note_session_commit_epoch(
        &self,
        oid: OID,
        watermark: Option<catalog::contract::epoch_oracle::CommitEpoch>,
    ) -> RS<()> {
        if let Some(watermark) = watermark {
            self.session_manager
                .note_session_commit_epoch(oid, watermark)?;
        }
        Ok(())
    }

    async fn run_sql_query_with_tx(
        &self,
        core: Arc<MuduConnCore>,
        current_schema: &str,
        stmt: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
        tx_mgr: Arc<dyn TxMgr>,
        autocommit: bool,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        let trace = task_trace!();
        trace.watch("sql.kind", "query");
        let text = stmt.to_sql_string();
        let catalog_version = self.meta_mgr().catalog_version();
        // Plan-cache hit: fill the cached template's parameter slots and run
        // it (point reads issue one `read_key`; everything else goes through
        // the planner) — parsing and binding are skipped entirely. The cache
        // key carries the schema because the same SQL text binds different
        // tables under different application schemas.
        if let Some(plan) = self
            .plan_cache
            .get(current_schema, &text, catalog_version)?
        {
            trace.watch("sql.stage", "plan_cache_hit");
            let result = plan
                .run_query(
                    param.as_ref(),
                    tx_mgr,
                    self.contract.clone(),
                    self.meta_mgr(),
                    self.contract.async_runtime(),
                )
                .await;
            trace.watch("sql.stage", if result.is_ok() { "done" } else { "error" });
            let (rows, desc) = result?;
            return Ok(Arc::new(MuduResultSetAsync::from_rows(rows, desc)));
        }
        trace.watch("sql.stage", "parse");
        let stmt = {
            let _stage =
                ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::SqlParse);
            core.parse_one_text(&text)?
        };
        // A queue RECEIVE carrying WAIT parks the statement on the queue wait
        // registry between short claim transactions; inside an explicit
        // transaction the client owns the commit, so the park would break the
        // statement's claim atomicity. WAIT is autocommit-only. RECEIVE is
        // never plan-cached, so this check always sees the parsed statement.
        if !autocommit && ::sql::binder::is_receive_wait_stmt(&stmt) {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "RECEIVE ... WAIT is only allowed in autocommit mode; it cannot run inside an explicit transaction"
            ));
        }
        // Miss: bind in template mode (no parameter values read), cache the
        // template, and execute it — the first execution pays one bind, same
        // as the uncached path.
        let template = {
            let _stage =
                ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::SqlBind);
            Binder::new(self.meta_mgr(), current_schema.to_string())
                .bind_template(&stmt)
                .await?
        };
        let Some(template) = template else {
            // DDL/COPY statements are never templated; take the regular path.
            trace.watch("sql.stage", "query");
            let result = core
                .query(&stmt, param, tx_mgr, self.contract.clone(), current_schema)
                .await;
            trace.watch("sql.stage", if result.is_ok() { "done" } else { "error" });
            return result;
        };
        // Cache only when the catalog did not change while binding; the
        // version-tagged entry would never be hit after a concurrent DDL, so
        // skipping the insert avoids churning the cache.
        let plan = if self.meta_mgr().catalog_version() == catalog_version {
            self.plan_cache
                .insert(current_schema, text, catalog_version, template)?
        } else {
            Arc::new(CachedPlan::new(template))
        };
        trace.watch("sql.stage", "query");
        let result = plan
            .run_query(
                param.as_ref(),
                tx_mgr,
                self.contract.clone(),
                self.meta_mgr(),
                self.contract.async_runtime(),
            )
            .await;
        trace.watch("sql.stage", if result.is_ok() { "done" } else { "error" });
        let (rows, desc) = result?;
        Ok(Arc::new(MuduResultSetAsync::from_rows(rows, desc)))
    }

    async fn run_sql_execute_with_tx(
        &self,
        core: Arc<MuduConnCore>,
        current_schema: &str,
        stmt: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
        tx_mgr: Arc<dyn TxMgr>,
    ) -> RS<u64> {
        let trace = task_trace!();
        trace.watch("procedure.worker_sql_execute.stage", "plan_cache_lookup");
        let text = stmt.to_sql_string();
        let catalog_version = self.meta_mgr().catalog_version();
        // See `run_sql_query_with_tx` for the cache semantics.
        if let Some(plan) = self
            .plan_cache
            .get(current_schema, &text, catalog_version)?
        {
            trace.watch("procedure.worker_sql_execute.stage", "plan_cache_hit");
            let result = plan
                .run_execute(
                    param.as_ref(),
                    tx_mgr,
                    self.contract.clone(),
                    self.meta_mgr(),
                    self.contract.async_runtime(),
                )
                .await;
            trace.watch(
                "procedure.worker_sql_execute.stage",
                if result.is_ok() {
                    "execute_done"
                } else {
                    "execute_error"
                },
            );
            return result;
        }
        trace.watch("procedure.worker_sql_execute.stage", "parse_start");
        let stmt = {
            let _stage =
                ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::SqlParse);
            core.parse_one_text(&text)?
        };
        // CREATE CACHE registers node-local volatile state that never travels
        // the replicated control plane; on a cluster node it would silently
        // diverge between members, so reject it explicitly instead of writing
        // only the local catalog.
        self.reject_cluster_create_cache(::sql::binder::is_create_cache_stmt(&stmt))?;
        let template = {
            let _stage =
                ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::SqlBind);
            Binder::new(self.meta_mgr(), current_schema.to_string())
                .bind_template(&stmt)
                .await?
        };
        let Some(template) = template else {
            // DDL/COPY statements are never templated; take the regular path.
            trace.watch("procedure.worker_sql_execute.stage", "execute_start");
            let result = core
                .execute(&stmt, param, tx_mgr, self.contract.clone(), current_schema)
                .await;
            trace.watch(
                "procedure.worker_sql_execute.stage",
                if result.is_ok() {
                    "execute_done"
                } else {
                    "execute_error"
                },
            );
            return result;
        };
        let plan = if self.meta_mgr().catalog_version() == catalog_version {
            self.plan_cache
                .insert(current_schema, text, catalog_version, template)?
        } else {
            Arc::new(CachedPlan::new(template))
        };
        trace.watch("procedure.worker_sql_execute.stage", "execute_start");
        let result = plan
            .run_execute(
                param.as_ref(),
                tx_mgr,
                self.contract.clone(),
                self.meta_mgr(),
                self.contract.async_runtime(),
            )
            .await;
        trace.watch(
            "procedure.worker_sql_execute.stage",
            if result.is_ok() {
                "execute_done"
            } else {
                "execute_error"
            },
        );
        result
    }

    /// Fetch the session's transaction after a possible statement-begin; a
    /// lookup failure with `started_tx` set aborts the just-registered
    /// transaction so it is not leaked.
    async fn session_tx_mgr_or_cleanup(&self, oid: OID, started_tx: bool) -> RS<Arc<dyn TxMgr>> {
        match self.sql_tx_mgr(oid) {
            Ok(Some(tx_mgr)) => Ok(tx_mgr),
            other => {
                if started_tx {
                    let tx_manager = self.session_manager.take_session_tx(oid)?;
                    let _ = self.contract.worker_abort_tx_async(tx_manager).await;
                }
                match other {
                    Ok(_) => Err(mudu_error!(
                        ErrorCode::Internal,
                        "session transaction is missing"
                    )),
                    Err(err) => Err(err),
                }
            }
        }
    }

    /// Commit (when `ok`) or abort the session transaction this statement
    /// started; a successful commit records the session's read-your-writes
    /// watermark. Commit errors abort the transaction before propagating (a
    /// leaked transaction pins the cluster epoch barrier).
    async fn settle_started_session_tx(&self, oid: OID, ok: bool) -> RS<()> {
        let tx_manager = self.session_manager.take_session_tx(oid)?;
        if !ok {
            self.contract.worker_abort_tx_async(tx_manager).await?;
            return Ok(());
        }
        match self
            .contract
            .worker_commit_routed_tx_async(tx_manager.clone())
            .await
        {
            Ok(committed) => self.note_session_commit_epoch(oid, committed),
            Err(err) => {
                let _ = self.contract.worker_abort_tx_async(tx_manager).await;
                Err(err)
            }
        }
    }
}

impl WorkerRuntime {
    /// Run a SQL query and return its result set.
    ///
    /// Transaction-control text (BEGIN/COMMIT/ROLLBACK) drives the session
    /// transaction lifecycle instead of the SQL pipeline. Any other
    /// statement runs inside the session transaction when one is active,
    /// otherwise in a fresh autocommit transaction. `oid` selects the
    /// session; 0 runs without a session.
    /// Run a SQL query and return its result set.
    ///
    /// Transaction and session handling mirrors [`Self::execute`]; commit
    /// errors abort the transaction before propagating (a leaked
    /// transaction pins the cluster epoch barrier).
    pub async fn query(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        if let Some(control) = parse_tx_control_stmt(&sql.to_sql_string()) {
            self.execute_tx_control_stmt(oid, control).await?;
            return Ok(Arc::new(MuduResultSetAsync::from_rows(
                vec![],
                TupleFieldDesc::new(vec![]),
            )));
        }
        let current_schema = current_schema(app_name);
        let core = self.sql_core(oid)?;
        if oid == 0 {
            let tx_mgr = self.contract.begin_tx().await?;
            let result = self
                .run_sql_query_with_tx(core, &current_schema, sql, param, tx_mgr.clone(), true)
                .await;
            if result.is_ok() {
                if let Err(err) = self.contract.commit_tx(tx_mgr.clone()).await {
                    let _ = self.contract.abort_tx(tx_mgr).await;
                    return Err(err);
                }
            } else {
                self.contract.abort_tx(tx_mgr).await?;
            }
            return result;
        }
        // Autocommit `RECEIVE ... WAIT` runs on an untracked statement
        // shell instead of a registered transaction: its work happens in
        // the wait loop's autonomous round transactions, and a held epoch
        // registration would pin the collecting epoch for the whole park —
        // producers' commits would stamp the pinned epoch, invisible to the
        // waiter's rounds, and their read-your-writes waits would hang on
        // the parked statement.
        if !self.session_manager.has_session_tx(oid)?
            && self.classify_receive_wait(&core, sql.as_ref())?
        {
            let shell = self.contract.worker_begin_tx_untracked()?;
            let result = self
                .run_sql_query_with_tx(core, &current_schema, sql, param, shell.clone(), true)
                .await;
            // The shell stays empty by construction; end it and record the
            // statement's watermark so the session's next transaction
            // observes the round commits (e.g. a following ACK).
            let watermark = self.contract.worker_current_epoch();
            let _ = self.contract.worker_abort_tx_async(shell).await;
            if result.is_ok() {
                self.note_session_commit_epoch(oid, watermark)?;
            }
            return result;
        }
        let started_tx = if self.session_manager.has_session_tx(oid)? {
            false
        } else {
            self.wait_session_commit_epoch(oid).await?;
            self.session_manager
                .begin_session_tx(oid, self.contract.worker_begin_tx()?)?;
            true
        };
        let tx_mgr = self.session_tx_mgr_or_cleanup(oid, started_tx).await?;
        let result = self
            .run_sql_query_with_tx(core, &current_schema, sql, param, tx_mgr, started_tx)
            .await;
        if started_tx {
            self.settle_started_session_tx(oid, result.is_ok()).await?;
        }
        result
    }

    /// Run a batch of SQL statements as one transaction and return the
    /// total number of affected rows.
    pub(crate) async fn batch(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        if param.size() != 0 {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "batch with parameters is not implemented"
            ));
        }
        let current_schema = current_schema(app_name);
        let core = self.sql_core(oid)?;
        let stmts = core.parse_many(sql.as_ref())?;
        // The batch path executes statements directly (bypassing
        // `run_sql_execute_with_tx`), so the cluster guard runs here too.
        self.reject_cluster_create_cache(stmts.iter().any(::sql::binder::is_create_cache_stmt))?;
        if oid == 0 {
            let tx_mgr = self.contract.begin_tx().await?;
            let mut total = 0;
            for stmt in stmts {
                match core
                    .execute(
                        &stmt,
                        Box::new(()),
                        tx_mgr.clone(),
                        self.contract.clone(),
                        &current_schema,
                    )
                    .await
                {
                    Ok(affected) => total += affected,
                    Err(err) => {
                        self.contract.abort_tx(tx_mgr).await?;
                        return Err(err);
                    }
                }
            }
            if let Err(err) = self.contract.commit_tx(tx_mgr.clone()).await {
                let _ = self.contract.abort_tx(tx_mgr).await;
                return Err(err);
            }
            return Ok(total);
        }
        let started_tx = if self.session_manager.has_session_tx(oid)? {
            false
        } else {
            self.wait_session_commit_epoch(oid).await?;
            self.session_manager
                .begin_session_tx(oid, self.contract.worker_begin_tx()?)?;
            true
        };
        let tx_mgr = self.session_tx_mgr_or_cleanup(oid, started_tx).await?;
        let mut total = 0;
        for stmt in stmts {
            match core
                .execute(
                    &stmt,
                    Box::new(()),
                    tx_mgr.clone(),
                    self.contract.clone(),
                    &current_schema,
                )
                .await
            {
                Ok(affected) => total += affected,
                Err(err) => {
                    if started_tx {
                        self.settle_started_session_tx(oid, false).await?;
                    }
                    return Err(err);
                }
            }
        }
        if started_tx {
            self.settle_started_session_tx(oid, true).await?;
        }
        Ok(total)
    }

    /// Run a SQL statement and return the number of affected rows.
    ///
    /// Transaction and session handling mirrors [`Self::query`]; commit
    /// errors abort the transaction before propagating (a leaked
    /// transaction pins the cluster epoch barrier).
    pub async fn execute(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        let trace = task_trace!();
        trace.watch("procedure.worker_execute.stage", "enter");
        trace.watch("procedure.worker_execute.oid", &oid.to_string());
        if let Some(control) = parse_tx_control_stmt(&sql.to_sql_string()) {
            self.execute_tx_control_stmt(oid, control).await?;
            return Ok(0);
        }
        let current_schema = current_schema(app_name);
        let core = self.sql_core(oid)?;
        if oid == 0 {
            trace.watch("procedure.worker_execute.stage", "begin_tx_start");
            let tx_mgr = self.contract.begin_tx().await?;
            trace.watch("procedure.worker_execute.stage", "begin_tx_done");
            let result = self
                .run_sql_execute_with_tx(core, &current_schema, sql, param, tx_mgr.clone())
                .await;
            if result.is_ok() {
                trace.watch("procedure.worker_execute.stage", "commit_start");
                if let Err(err) = self.contract.commit_tx(tx_mgr.clone()).await {
                    let _ = self.contract.abort_tx(tx_mgr).await;
                    return Err(err);
                }
                trace.watch("procedure.worker_execute.stage", "commit_done");
            } else {
                trace.watch("procedure.worker_execute.stage", "abort_start");
                self.contract.abort_tx(tx_mgr).await?;
                trace.watch("procedure.worker_execute.stage", "abort_done");
            }
            return result;
        }
        let result = self
            .execute_in_session(oid, core, &current_schema, sql, param)
            .await;
        trace.watch(
            "procedure.worker_execute.stage",
            if result.is_ok() { "done" } else { "error" },
        );
        result
    }

    /// The session-bound branch of [`Self::execute`]: joins the session's
    /// open transaction or begins one for this statement (then commits or
    /// aborts it). Commit errors abort the transaction before propagating.
    async fn execute_in_session(
        &self,
        oid: OID,
        core: Arc<MuduConnCore>,
        current_schema: &str,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        let trace = task_trace!();
        let started_tx = if self.session_manager.has_session_tx(oid)? {
            false
        } else {
            trace.watch("procedure.worker_execute.stage", "session_begin_tx_start");
            self.wait_session_commit_epoch(oid).await?;
            self.session_manager
                .begin_session_tx(oid, self.contract.worker_begin_tx()?)?;
            trace.watch("procedure.worker_execute.stage", "session_begin_tx_done");
            true
        };
        // A failure between the begin and the commit/abort below must not
        // leak the just-registered transaction.
        let tx_mgr = match self.sql_tx_mgr(oid) {
            Ok(Some(tx_mgr)) => tx_mgr,
            other => {
                if started_tx {
                    let tx_manager = self.session_manager.take_session_tx(oid)?;
                    let _ = self.contract.worker_abort_tx_async(tx_manager).await;
                }
                return match other {
                    Ok(_) => Err(mudu_error!(
                        ErrorCode::Internal,
                        "session transaction is missing"
                    )),
                    Err(err) => Err(err),
                };
            }
        };
        trace.watch("procedure.worker_execute.stage", "run_sql_execute_start");
        let result = self
            .run_sql_execute_with_tx(core, current_schema, sql, param, tx_mgr)
            .await;
        if started_tx {
            let tx_manager = self.session_manager.take_session_tx(oid)?;
            if result.is_ok() {
                trace.watch("procedure.worker_execute.stage", "session_commit_start");
                match self
                    .contract
                    .worker_commit_routed_tx_async(tx_manager.clone())
                    .await
                {
                    Ok(committed) => self.note_session_commit_epoch(oid, committed)?,
                    Err(err) => {
                        let _ = self.contract.worker_abort_tx_async(tx_manager).await;
                        return Err(err);
                    }
                }
                trace.watch("procedure.worker_execute.stage", "session_commit_done");
            } else {
                trace.watch("procedure.worker_execute.stage", "session_rollback_start");
                self.contract.worker_abort_tx_async(tx_manager).await?;
                trace.watch("procedure.worker_execute.stage", "session_rollback_done");
            }
        }
        result
    }
}
