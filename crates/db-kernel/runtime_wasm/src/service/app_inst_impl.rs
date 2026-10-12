use super::invocation_admission::Admission;
use super::invocation_limits::{CancelOnDrop, InvocationContext, InvocationLimits};
use super::procedure_invoke_component::ProcOpt;
use crate::procedure::procedure::Procedure;
use crate::resolver::schema_mgr::SchemaMgr;
use crate::service::app_inst::AppInst;
use crate::service::app_package::AppPackage;
use crate::service::package_module::PackageModule;
use crate::service::procedure_invoke_component::ProcedureInvokeComponent;
use crate::service::runtime_opt::ComponentTarget;
use crate::service::runtime_opt::ServerMode;
use ::conn::worker_local::WorkerLocalRef;
use async_trait::async_trait;
use mudu::common::app_info::AppInfo;
use mudu::common::result::RS;
use mudu::common::xid::is_xid_invalid;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::sql::{Context, DBConn};
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::fs;
use mudu_sys::sync::SMutex;
use mudu_utils::task_id::{TaskID, new_task_id};
use mudu_utils::task_trace;
use runtime_libsql::db_connector::DBConnector;
use scc::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// A loaded Mudu application instance: package configuration, schema,
/// compiled modules, and per-task database connections.
#[derive(Clone)]
pub struct AppInstImpl {
    inner: Arc<AppInstImplInner>,
}

struct AppInstImplInner {
    package_cfg: AppInfo,
    enable_async: bool,
    server_mode: ServerMode,
    db_path: String,
    schema_mgr: SchemaMgr,
    modules: HashMap<String, PackageModule>,
    _conn: HashMap<u128, DBConn>,
    component_target: ComponentTarget,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    /// Deferred initdb DDL text, present only when startup auto-load skipped
    /// executing it (see [`AppInstImpl::build`]). Cleared by
    /// [`AppInstImpl::drain_initdb`] once the DDL has been applied.
    pending_initdb: SMutex<Option<String>>,
    limits: InvocationLimits,
    legacy_gate: mudu_sys::sync::async_::AMutex<()>,
}

impl AppInstImpl {
    /// Builds an application instance from a loaded package.
    ///
    /// When `defer_initdb` is true and the app's initdb lock sentinel is
    /// missing, the initdb DDL is not executed during the build: kernel server
    /// startup runs this on threads that race the server's own TCP listener
    /// (the connection would get `ECONNREFUSED` from the not-yet-bound
    /// listener). The DDL text is recorded as pending instead, and
    /// [`AppInstImpl::drain_initdb`] applies it once the server is accepting
    /// connections.
    #[expect(
        clippy::too_many_arguments,
        reason = "package load options are threaded explicitly to stay parallel with the install and test call sites"
    )]
    pub async fn build(
        db_path: &str,
        package: &AppPackage,
        vec_modules: Vec<(String, PackageModule)>,
        component_target: ComponentTarget,
        enable_async: bool,
        server_mode: ServerMode,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
        defer_initdb: bool,
    ) -> RS<Self> {
        Ok(Self {
            inner: Arc::new(
                AppInstImplInner::build(
                    db_path,
                    package,
                    vec_modules,
                    component_target,
                    enable_async,
                    server_mode,
                    async_runtime,
                    defer_initdb,
                )
                .await?,
            ),
        })
    }

    /// Installs validated server limits before the application is published.
    pub fn with_limits(mut self, limits: InvocationLimits) -> RS<Self> {
        limits.validate()?;
        Arc::get_mut(&mut self.inner)
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "application already shared"))?
            .limits = limits;
        Ok(self)
    }

    /// Returns the database connection registered for a task, if any.
    pub fn connection(&self, task_id: u128) -> Option<DBConn> {
        self.inner.connection(task_id)
    }

    /// Creates and registers a database connection for a task.
    pub async fn create_conn(&self, task_id: u128) -> RS<()> {
        self.inner.create_conn(task_id).await
    }

    /// Removes the database connection registered for a task.
    pub fn remove_conn(&self, task_id: u128) -> RS<()> {
        self.inner.remove_conn(task_id)
    }

    /// Looks up a procedure by module and procedure name.
    pub fn procedure(&self, mod_name: &str, proc_name: &str) -> Option<Procedure> {
        self.inner.procedure(mod_name, proc_name)
    }

    /// Returns the application name.
    pub fn name(&self) -> &String {
        self.inner.name()
    }

    /// Returns the application's schema manager.
    pub fn schema_mgr(&self) -> &SchemaMgr {
        self.inner.schema_mgr()
    }

    /// Returns the configured async I/O provider, if any.
    pub fn async_runtime(&self) -> Option<Arc<dyn AsyncIoProvider>> {
        self.inner.async_runtime()
    }

    /// Applies deferred initdb DDL, if any is pending.
    ///
    /// Runs the same `initdb` logic as an immediate build: the lock sentinel
    /// is re-checked (a concurrent drain may have completed it), the DDL is
    /// applied over a fresh connection, and the sentinel is written only on
    /// success. The pending flag is cleared only after success, so a failed
    /// drain can be retried later.
    pub async fn drain_initdb(&self) -> RS<()> {
        self.inner.drain_initdb().await
    }
}

impl AppInstImplInner {
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors AppInstImpl::build, which carries the same package load options"
    )]
    async fn build(
        db_path: &str,
        package: &AppPackage,
        vec_modules: Vec<(String, PackageModule)>,
        component_target: ComponentTarget,
        enable_async: bool,
        server_mode: ServerMode,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
        defer_initdb: bool,
    ) -> RS<Self> {
        let modules = HashMap::new();
        let app_cfg = &package.package_cfg;
        let ddl_sql = &package.ddl_sql;
        let init_sql = &package.initdb_sql;
        let schema_mgr = SchemaMgr::from_sql_text(ddl_sql)?;
        for (name, module) in vec_modules {
            let _ = modules.insert_sync(name, module);
        }
        SchemaMgr::add_mgr(app_cfg.name.clone(), schema_mgr.clone());
        let sql_text = ddl_sql.to_string() + init_sql.as_str();
        // Deferred mode (kernel server startup auto-load): skip the initdb
        // connection when the DDL has not been applied yet; the drain runs it
        // once the server is accepting connections. The lock-sentinel check
        // mirrors the early return in `initdb` below.
        let pending_initdb = if defer_initdb
            && !fs::sync::sync_path_exists(initdb_lock_path(db_path, &app_cfg.name))
        {
            Some(sql_text)
        } else {
            initdb(
                db_path,
                &app_cfg.name,
                &sql_text,
                &schema_mgr,
                enable_async,
                server_mode,
                async_runtime.clone(),
            )
            .await?;
            None
        };
        Ok(Self {
            package_cfg: app_cfg.clone(),
            enable_async,
            server_mode,
            db_path: db_path.to_owned(),
            schema_mgr,
            modules,
            _conn: Default::default(),
            component_target,
            async_runtime,
            pending_initdb: SMutex::new(pending_initdb),
            limits: InvocationLimits::default(),
            legacy_gate: mudu_sys::sync::async_::AMutex::new(()),
        })
    }

    pub fn list_procedure(&self) -> RS<Vec<(String, String)>> {
        let mut vec = Vec::new();
        self.modules.iter_sync(|_k, v| {
            let mod_proc_list = v.procedure_list();
            vec.extend(mod_proc_list.iter().cloned());
            true
        });
        Ok(vec)
    }
    pub fn describe_procedure(&self, mod_name: &str, proc_name: &str) -> RS<Arc<ProcDesc>> {
        let procedure = self.procedure(mod_name, proc_name).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such module named {} {}", mod_name, proc_name)
            )
        })?;
        Ok(procedure.desc())
    }

    async fn invoke_managed(
        self: &Arc<Self>,
        task_id: TaskID,
        mod_name: &str,
        proc_name: &str,
        param: ProcedureParam,
        worker: Option<WorkerLocalRef>,
        use_async: bool,
    ) -> RS<ProcedureResult> {
        if use_async && !self.enable_async {
            return Err(mudu_error!(
                ErrorCode::Database,
                "enable async mode when call async procedure"
            ));
        }
        // Acquire before transaction setup. The managed task retains the permit
        // even when the client drops its response future.
        let permit = Admission::acquire(&self.package_cfg.name, &self.limits)?;
        let control = InvocationContext::new(self.limits.clone())?;
        let mut cancel = CancelOnDrop(Some(control.clone()));
        let inner = self.clone();
        let module = mod_name.to_owned();
        let procedure = proc_name.to_owned();
        let (notify, wait) = mudu_sys::sync::async_::create_notify_wait();
        super::invocation_task::spawn(async move {
            let permit = permit;
            // SQLite-backed legacy apps cannot upgrade concurrent deferred
            // read transactions to writers. Serialize within this app only.
            let _legacy = if inner.server_mode == ServerMode::Legacy {
                Some(inner.legacy_gate.lock().await)
            } else {
                None
            };
            let result = inner
                .run_invocation(
                    task_id, &module, &procedure, param, worker, use_async, &control,
                )
                .await;
            if control.is_cancelled() {
                let _ = inner.remove_conn(task_id);
            }
            drop(permit);
            let _ = notify.notify(result);
        })?;
        let result = wait
            .wait()
            .await?
            .ok_or_else(|| mudu_error!(ErrorCode::Thread, "procedure lifecycle stopped"))?;
        cancel.0 = None;
        result
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "one lifecycle binds the task, procedure, worker and cancellation state"
    )]
    async fn run_invocation(
        &self,
        task_id: TaskID,
        mod_name: &str,
        proc_name: &str,
        param: ProcedureParam,
        worker: Option<WorkerLocalRef>,
        use_async: bool,
        control: &Arc<InvocationContext>,
    ) -> RS<ProcedureResult> {
        control.check()?;
        super::invocation_query::check_size(param.param_list(), control.limits.max_input_bytes)?;
        let (procedure, param, new_tx) =
            self.pre_invoke(task_id, mod_name, proc_name, param).await?;
        let xid = param.session_id();
        let opt = ProcOpt::with_context(control.clone());
        let mut result = if let Err(error) = control.check() {
            Err(error)
        } else if use_async {
            ProcedureInvokeComponent::call_async(
                &procedure,
                self.component_target,
                opt,
                param,
                worker,
            )
            .await
        } else {
            ProcedureInvokeComponent::call_on_executor(
                procedure,
                self.component_target,
                opt,
                param,
                worker,
            )
            .await
        };
        if result.is_ok()
            && let Err(error) = control.check()
        {
            result = Err(error);
        }
        if let Err(error) = control.cleanup_resources().await {
            result = Err(error);
        }
        if new_tx {
            if result.is_ok()
                && let Err(error) = control.begin_commit()
            {
                result = Err(error);
            }
            // Once committing, cancellation no longer changes the outcome. A
            // commit error is returned as-is, never mislabeled as a rollback.
            let conn_is_async = matches!(self.connection(task_id), Some(DBConn::Async(_)));
            let finish = match (result.is_ok(), conn_is_async) {
                (true, true) => Context::commit_async(xid).await,
                (false, true) => Context::rollback_async(xid).await,
                (true, false) => Context::commit(xid),
                (false, false) => Context::rollback(xid),
            };
            Context::remove(xid);
            if let Err(error) = finish {
                let _ = self.remove_conn(task_id);
                return Err(mudu_error!(
                    ErrorCode::Transaction,
                    if result.is_ok() {
                        "procedure commit failed; outcome may be unknown"
                    } else {
                        "procedure rollback failed; connection discarded"
                    },
                    error
                ));
            }
        } else if result.is_err()
            && let Some(context) = Context::context(xid)
        {
            context.mark_failed();
        }
        result
    }

    pub fn procedure(&self, mod_name: &str, proc_name: &str) -> Option<Procedure> {
        self.modules.get_sync(mod_name)?.get().procedure(proc_name)
    }

    pub async fn create_conn(&self, task_id: u128) -> RS<()> {
        let db_conn = new_conn(
            &self.db_path,
            &self.package_cfg.name,
            self.enable_async,
            self.server_mode,
            self.async_runtime(),
        )
        .await?;
        self._conn.insert_sync(task_id, db_conn).map_err(|_e| {
            mudu_error!(
                ErrorCode::EntityAlreadyExists,
                format!("existing such task {} connection", task_id)
            )
        })?;
        Ok(())
    }

    pub fn remove_conn(&self, task_id: u128) -> RS<()> {
        let _ = self._conn.remove_sync(&task_id);
        Ok(())
    }
    pub fn connection(&self, task_id: u128) -> Option<DBConn> {
        self._conn.get_sync(&task_id).map(|conn| conn.clone())
    }

    pub fn name(&self) -> &String {
        &self.package_cfg.name
    }

    pub fn schema_mgr(&self) -> &SchemaMgr {
        &self.schema_mgr
    }

    pub fn async_runtime(&self) -> Option<Arc<dyn AsyncIoProvider>> {
        self.async_runtime.clone()
    }

    async fn drain_initdb(&self) -> RS<()> {
        let sql_text = self.pending_initdb.lock()?.clone();
        let Some(sql_text) = sql_text else {
            return Ok(());
        };
        initdb(
            &self.db_path,
            &self.package_cfg.name,
            &sql_text,
            &self.schema_mgr,
            self.enable_async,
            self.server_mode,
            self.async_runtime.clone(),
        )
        .await?;
        *self.pending_initdb.lock()? = None;
        Ok(())
    }

    async fn pre_invoke(
        &self,
        task_id: TaskID,
        mod_name: &str,
        proc_name: &str,
        param: ProcedureParam,
    ) -> RS<(Procedure, ProcedureParam, bool)> {
        let trace = task_trace!();
        trace.watch("procedure.pre_invoke.stage", "lookup");
        let procedure = self.procedure(mod_name, proc_name).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("procedure {}/{} not found", mod_name, proc_name)
            )
        })?;

        let existing_xid = param.session_id();
        let (param, new_tx) = if is_xid_invalid(&existing_xid) {
            let conn = self.connection(task_id).ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("no such task named {}", task_id)
                )
            })?;
            // Bind the context to this application's schema so guest
            // relation syscalls resolve bare table names in it.
            let context = Context::create_with_app(task_id, conn, self.package_cfg.name.clone())?;
            let mut param = param;
            param.set_session_id(context.session_id());
            trace.watch("procedure.pre_invoke.stage", "begin_tx_start");
            if let Err(error) = context.begin_tx().await {
                Context::remove(context.session_id());
                return Err(error);
            }
            trace.watch("procedure.pre_invoke.stage", "begin_tx_done");
            (param, true)
        } else {
            (param, false)
        };
        trace.watch("procedure.pre_invoke.stage", "done");
        Ok((procedure, param, new_tx))
    }
}

async fn new_conn(
    db_path: &str,
    app_name: &str,
    enable_async: bool,
    server_mode: ServerMode,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
) -> RS<DBConn> {
    let db_type = if matches!(server_mode, ServerMode::IOUring | ServerMode::Tokio) {
        "MuduDB".to_string()
    } else if enable_async {
        "LibSQLAsync".to_string()
    } else {
        "LibSQL".to_string()
    };
    let conn_str = format!("db={} app={} db_type={}", db_path, app_name, db_type);
    let db_conn = DBConnector::connect_with_async_runtime(&conn_str, async_runtime).await?;
    Ok(db_conn)
}

fn initdb_lock_path(db_path: &str, app_name: &str) -> PathBuf {
    PathBuf::from(&db_path).join(format!("{}.lock", app_name))
}

async fn initdb(
    db_path: &str,
    app_name: &str,
    sql: &str,
    schema_mgr: &SchemaMgr,
    enable_async: bool,
    server_mode: ServerMode,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
) -> RS<()> {
    let init_db_lock = initdb_lock_path(db_path, app_name);
    if fs::sync::sync_path_exists(&init_db_lock) {
        if matches!(server_mode, ServerMode::IOUring | ServerMode::Tokio) {
            return Ok(());
        }
        if is_schema_initialized(
            db_path,
            app_name,
            schema_mgr,
            enable_async,
            server_mode,
            async_runtime.clone(),
        )
        .await?
        {
            return Ok(());
        }
    }
    let conn = new_conn(db_path, app_name, enable_async, server_mode, async_runtime).await?;
    conn.execute_silent(sql.to_owned()).await?;
    fs::sync::SFile::create(&init_db_lock)?;
    Ok(())
}

async fn is_schema_initialized(
    db_path: &str,
    app_name: &str,
    schema_mgr: &SchemaMgr,
    enable_async: bool,
    server_mode: ServerMode,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
) -> RS<bool> {
    let conn = new_conn(db_path, app_name, enable_async, server_mode, async_runtime).await?;
    for table_name in schema_mgr.table_names() {
        let verify_sql = format!("SELECT 1 FROM {} LIMIT 1;", table_name);
        if conn.execute_silent(verify_sql).await.is_err() {
            return Ok(false);
        }
    }
    Ok(true)
}

#[async_trait]
impl AppInst for AppInstImpl {
    fn cfg(&self) -> &AppInfo {
        &self.inner.package_cfg
    }

    async fn task_create(&self) -> RS<TaskID> {
        let id = new_task_id();
        self.create_conn(id).await?;
        Ok(id)
    }

    fn task_end(&self, task_id: TaskID) -> RS<()> {
        self.remove_conn(task_id)
    }

    fn connection(&self, task_id: TaskID) -> Option<DBConn> {
        self.inner.connection(task_id)
    }

    fn procedure(&self) -> RS<Vec<(String, String)>> {
        self.inner.list_procedure()
    }

    async fn invoke(
        &self,
        task_id: TaskID,
        mod_name: &str,
        proc_name: &str,
        param: ProcedureParam,
        worker_local: Option<WorkerLocalRef>,
    ) -> RS<ProcedureResult> {
        self.inner
            .invoke_managed(task_id, mod_name, proc_name, param, worker_local, false)
            .await
    }

    async fn invoke_async(
        &self,
        task_id: TaskID,
        mod_name: &str,
        proc_name: &str,
        param: ProcedureParam,
        worker_local: Option<WorkerLocalRef>,
    ) -> RS<ProcedureResult> {
        self.inner
            .invoke_managed(task_id, mod_name, proc_name, param, worker_local, true)
            .await
    }

    fn describe(&self, mod_name: &str, proc_name: &str) -> RS<Arc<ProcDesc>> {
        self.inner.describe_procedure(mod_name, proc_name)
    }
}

// All tests in this module exercise libsql's real file IO (see the module
// file's comments); skip compiling it under the deterministic backend.
#[cfg(all(test, not(feature = "ds")))]
#[path = "app_inst_impl_test.rs"]
mod app_inst_impl_test;
