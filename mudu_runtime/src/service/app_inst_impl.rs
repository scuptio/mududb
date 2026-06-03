use crate::backend::mududb_cfg::ServerMode;
use crate::db_connector::DBConnector;
use crate::procedure::procedure::Procedure;
use crate::resolver::schema_mgr::SchemaMgr;
use crate::service::app_inst::AppInst;
use crate::service::mudu_package::MuduPackage;
use crate::service::package_module::PackageModule;
use crate::service::procedure_invoke_component::ProcedureInvokeComponent;
use crate::service::runtime_opt::ComponentTarget;
use async_trait::async_trait;
use mudu::common::app_info::AppInfo;
use mudu::common::result::RS;
use mudu::common::xid::is_xid_invalid;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::sql::{Context, DBConn};
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use mudu_sys::async_rt::contract::AsyncRuntime;
use mudu_kernel::server::worker_local::WorkerLocalRef;
use mudu_utils::task_id::{TaskID, new_task_id};
use mudu_utils::task_trace;
use scc::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

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
    async_runtime: Option<Arc<dyn AsyncRuntime>>,
}

impl AppInstImpl {
    pub async fn build(
        db_path: &String,
        package: &MuduPackage,
        vec_modules: Vec<(String, PackageModule)>,
        component_target: ComponentTarget,
        enable_async: bool,
        server_mode: ServerMode,
        async_runtime: Option<Arc<dyn AsyncRuntime>>,
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
                )
                .await?,
            ),
        })
    }

    pub fn connection(&self, task_id: u128) -> Option<DBConn> {
        self.inner.connection(task_id)
    }

    pub async fn create_conn(&self, task_id: u128) -> RS<()> {
        self.inner.create_conn(task_id).await
    }

    pub fn remove_conn(&self, task_id: u128) -> RS<()> {
        self.inner.remove_conn(task_id)
    }

    pub fn procedure(&self, mod_name: &str, proc_name: &str) -> Option<Procedure> {
        self.inner.procedure(mod_name, proc_name)
    }

    pub fn name(&self) -> &String {
        self.inner.name()
    }

    pub fn schema_mgr(&self) -> &SchemaMgr {
        &self.inner.schema_mgr()
    }

    pub fn async_runtime(&self) -> Option<Arc<dyn AsyncRuntime>> {
        self.inner.async_runtime()
    }
}

impl AppInstImplInner {
    async fn build(
        db_path: &String,
        package: &MuduPackage,
        vec_modules: Vec<(String, PackageModule)>,
        component_target: ComponentTarget,
        enable_async: bool,
        server_mode: ServerMode,
        async_runtime: Option<Arc<dyn AsyncRuntime>>,
    ) -> RS<Self> {
        let modules = HashMap::new();
        let app_cfg = &package.package_cfg;
        let ddl_sql = &package.ddl_sql;
        let init_sql = &package.initdb_sql;
        let schema_mgr = SchemaMgr::from_sql_text(&ddl_sql)?;
        for (name, module) in vec_modules {
            let _ = modules.insert_sync(name, module);
        }
        SchemaMgr::add_mgr(app_cfg.name.clone(), schema_mgr.clone());
        let sql_text = ddl_sql.to_string() + init_sql.as_str();
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
        Ok(Self {
            package_cfg: app_cfg.clone(),
            enable_async,
            server_mode,
            db_path: db_path.clone(),
            schema_mgr,
            modules,
            _conn: Default::default(),
            component_target,
            async_runtime,
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
    pub fn describe_procedure(&self, mod_name: &String, proc_name: &String) -> RS<Arc<ProcDesc>> {
        let procedure = self.procedure(mod_name, proc_name).ok_or_else(|| {
            m_error!(
                EC::NoneErr,
                format!("no such module named {} {}", mod_name, proc_name)
            )
        })?;
        Ok(procedure.desc())
    }

    pub async fn invoke_procedure(
        &self,
        task_id: TaskID,
        mod_name: &String,
        proc_name: &String,
        param: ProcedureParam,
        worker_local: Option<WorkerLocalRef>,
    ) -> RS<ProcedureResult> {
        let (procedure, param, new_tx) =
            self.pre_invoke(task_id, mod_name, proc_name, param).await?;
        let xid = param.session_id();
        let result = ProcedureInvokeComponent::call(
            &procedure,
            self.component_target,
            Default::default(),
            param,
            worker_local,
        );
        if new_tx {
            if result.is_ok() {
                Context::commit(xid)?;
            } else {
                Context::rollback(xid)?;
            }
        }
        Ok(result?)
    }

    pub async fn invoke_procedure_async(
        &self,
        task_id: TaskID,
        mod_name: &String,
        proc_name: &String,
        param: ProcedureParam,
        worker_local: Option<WorkerLocalRef>,
    ) -> RS<ProcedureResult> {
        let trace = task_trace!();
        trace.watch("procedure.mode", "async");
        trace.watch("procedure.task_id", &task_id.to_string());
        trace.watch("procedure.module", mod_name);
        trace.watch("procedure.name", proc_name);
        if !self.enable_async {
            return Err(m_error!(
                EC::DBInternalError,
                "enable async mode when call async procedure"
            ));
        }
        trace.watch("procedure.stage", "pre_invoke");
        let (procedure, param, new_tx) =
            self.pre_invoke(task_id, mod_name, proc_name, param).await?;
        let xid = param.session_id();
        trace.watch("procedure.session_id", &xid.to_string());
        trace.watch("procedure.stage", "call_async");
        let result = ProcedureInvokeComponent::call_async(
            &procedure,
            self.component_target,
            Default::default(),
            param,
            worker_local,
        )
        .await;
        if new_tx {
            if result.is_ok() {
                trace.watch("procedure.stage", "commit_async");
                Context::commit_async(xid).await?;
            } else {
                trace.watch("procedure.stage", "rollback_async");
                Context::rollback_async(xid).await?;
            }
        }
        trace.watch("procedure.stage", "done");
        Ok(result?)
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
            m_error!(
                EC::ExistingSuchElement,
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

    pub fn async_runtime(&self) -> Option<Arc<dyn AsyncRuntime>> {
        self.async_runtime.clone()
    }

    async fn pre_invoke(
        &self,
        task_id: TaskID,
        mod_name: &String,
        proc_name: &String,
        param: ProcedureParam,
    ) -> RS<(Procedure, ProcedureParam, bool)> {
        let trace = task_trace!();
        trace.watch("procedure.pre_invoke.stage", "lookup");
        let procedure = self.procedure(mod_name, proc_name).ok_or_else(|| {
            m_error!(
                EC::NoneErr,
                format!("procedure {}/{} not found", mod_name, proc_name)
            )
        })?;

        let existing_xid = param.session_id();
        let (param, new_tx) = if is_xid_invalid(&existing_xid) {
            let conn = self
                .connection(task_id)
                .ok_or_else(|| m_error!(EC::NoneErr, format!("no such task named {}", task_id)))?;
            let context = Context::create(task_id, conn)?;
            let mut param = param;
            param.set_session_id(context.session_id());
            trace.watch("procedure.pre_invoke.stage", "begin_tx_start");
            context.begin_tx().await?;
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
    db_path: &String,
    app_name: &String,
    enable_async: bool,
    server_mode: ServerMode,
    async_runtime: Option<Arc<dyn AsyncRuntime>>,
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

async fn initdb(
    db_path: &String,
    app_name: &String,
    sql: &String,
    schema_mgr: &SchemaMgr,
    enable_async: bool,
    server_mode: ServerMode,
    async_runtime: Option<Arc<dyn AsyncRuntime>>,
) -> RS<()> {
    let init_db_lock = PathBuf::from(&db_path).join(format!("{}.lock", app_name));
    if init_db_lock.exists() {
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
    conn.execute_silent(sql.clone()).await?;
    File::create(&init_db_lock).map_err(|e| {
        m_error!(
            EC::IOErr,
            format!("failed to create file: {}", init_db_lock.to_str().unwrap()),
            e
        )
    })?;
    Ok(())
}

async fn is_schema_initialized(
    db_path: &String,
    app_name: &String,
    schema_mgr: &SchemaMgr,
    enable_async: bool,
    server_mode: ServerMode,
    async_runtime: Option<Arc<dyn AsyncRuntime>>,
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
        mod_name: &String,
        proc_name: &String,
        param: ProcedureParam,
        worker_local: Option<WorkerLocalRef>,
    ) -> RS<ProcedureResult> {
        self.inner
            .invoke_procedure(task_id, mod_name, proc_name, param, worker_local)
            .await
    }

    async fn invoke_async(
        &self,
        task_id: TaskID,
        mod_name: &String,
        proc_name: &String,
        param: ProcedureParam,
        worker_local: Option<WorkerLocalRef>,
    ) -> RS<ProcedureResult> {
        self.inner
            .invoke_procedure_async(task_id, mod_name, proc_name, param, worker_local)
            .await
    }

    fn describe(&self, mod_name: &String, proc_name: &String) -> RS<Arc<ProcDesc>> {
        self.inner.describe_procedure(mod_name, proc_name)
    }
}
