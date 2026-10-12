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
    /// `EntityAlreadyExists` (no nesting); COMMIT goes through the routed
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

    async fn run_sql_query_with_tx(
        &self,
        core: Arc<MuduConnCore>,
        current_schema: &str,
        stmt: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
        tx_mgr: Arc<dyn TxMgr>,
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

    /// Run a SQL query and return its result set.
    ///
    /// Transaction-control text (BEGIN/COMMIT/ROLLBACK) drives the session
    /// transaction lifecycle instead of the SQL pipeline. Any other
    /// statement runs inside the session transaction when one is active,
    /// otherwise in a fresh autocommit transaction. `oid` selects the
    /// session; 0 runs without a session.
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
                .run_sql_query_with_tx(core, &current_schema, sql, param, tx_mgr.clone())
                .await;
            if result.is_ok() {
                self.contract.commit_tx(tx_mgr).await?;
            } else {
                self.contract.abort_tx(tx_mgr).await?;
            }
            return result;
        }
        let started_tx = if self.session_manager.has_session_tx(oid)? {
            false
        } else {
            self.session_manager
                .begin_session_tx(oid, self.contract.worker_begin_tx()?)?;
            true
        };
        let tx_mgr = self
            .sql_tx_mgr(oid)?
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "session transaction is missing"))?;
        let result = self
            .run_sql_query_with_tx(core, &current_schema, sql, param, tx_mgr)
            .await;
        if started_tx {
            let tx_manager = self.session_manager.take_session_tx(oid)?;
            if result.is_ok() {
                self.contract
                    .worker_commit_routed_tx_async(tx_manager)
                    .await?;
            } else {
                self.contract.worker_abort_tx_async(tx_manager).await?;
            }
        }
        result
    }

    /// Run a SQL statement and return the number of affected rows.
    ///
    /// Transaction and session handling mirrors [`Self::query`].
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
                self.contract.commit_tx(tx_mgr).await?;
                trace.watch("procedure.worker_execute.stage", "commit_done");
            } else {
                trace.watch("procedure.worker_execute.stage", "abort_start");
                self.contract.abort_tx(tx_mgr).await?;
                trace.watch("procedure.worker_execute.stage", "abort_done");
            }
            return result;
        }
        let started_tx = if self.session_manager.has_session_tx(oid)? {
            false
        } else {
            trace.watch("procedure.worker_execute.stage", "session_begin_tx_start");
            self.session_manager
                .begin_session_tx(oid, self.contract.worker_begin_tx()?)?;
            trace.watch("procedure.worker_execute.stage", "session_begin_tx_done");
            true
        };
        let tx_mgr = self
            .sql_tx_mgr(oid)?
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "session transaction is missing"))?;
        trace.watch("procedure.worker_execute.stage", "run_sql_execute_start");
        let result = self
            .run_sql_execute_with_tx(core, &current_schema, sql, param, tx_mgr)
            .await;
        if started_tx {
            let tx_manager = self.session_manager.take_session_tx(oid)?;
            if result.is_ok() {
                trace.watch("procedure.worker_execute.stage", "session_commit_start");
                self.contract
                    .worker_commit_routed_tx_async(tx_manager)
                    .await?;
                trace.watch("procedure.worker_execute.stage", "session_commit_done");
            } else {
                trace.watch("procedure.worker_execute.stage", "session_rollback_start");
                self.contract.worker_abort_tx_async(tx_manager).await?;
                trace.watch("procedure.worker_execute.stage", "session_rollback_done");
            }
        }
        trace.watch(
            "procedure.worker_execute.stage",
            if result.is_ok() { "done" } else { "error" },
        );
        result
    }

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
            self.contract.commit_tx(tx_mgr).await?;
            return Ok(total);
        }
        let started_tx = if self.session_manager.has_session_tx(oid)? {
            false
        } else {
            self.session_manager
                .begin_session_tx(oid, self.contract.worker_begin_tx()?)?;
            true
        };
        let tx_mgr = self
            .sql_tx_mgr(oid)?
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "session transaction is missing"))?;
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
                        let tx_manager = self.session_manager.take_session_tx(oid)?;
                        self.contract.worker_abort_tx_async(tx_manager).await?;
                    }
                    return Err(err);
                }
            }
        }
        if started_tx {
            let tx_manager = self.session_manager.take_session_tx(oid)?;
            self.contract
                .worker_commit_routed_tx_async(tx_manager)
                .await?;
        }
        Ok(total)
    }
}
