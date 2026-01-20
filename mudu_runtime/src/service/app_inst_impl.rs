use crate::db_connector::DBConnector;
use crate::procedure::procedure::Procedure;
use crate::resolver::schema_mgr::SchemaMgr;
use crate::service::app_inst::AppInst;
use crate::service::mudu_package::MuduPackage;
use crate::service::package_module::PackageModule;
use crate::service::procedure_invoke_p1::ProcedureInvoke1;
use crate::service::procedure_invoke_p2::ProcedureInvokeP2;
use async_trait::async_trait;
use mudu::common::package_cfg::PackageCfg;
use mudu::common::result::RS;
use mudu::common::xid::is_xid_invalid;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::sql::{Context, DBConn};
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use mudu_utils::task_id::{new_task_id, TaskID};
use scc::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppInstImpl {
    inner: Arc<AppInstImplInner>,
}

struct AppInstImplInner {
    package_cfg: PackageCfg,
    enable_async: bool,
    db_path: String,
    schema_mgr: SchemaMgr,
    modules: HashMap<String, PackageModule>,
    _conn: HashMap<u128, DBConn>,
    use_p2: bool,
}

impl AppInstImpl {
    pub async fn build(
        db_path: &String,
        package: &MuduPackage,
        vec_modules: Vec<(String, PackageModule)>,
        build_p2:bool,
        enable_async: bool,
    ) -> RS<Self> {
        Ok(Self {
            inner: Arc::new(AppInstImplInner::build(
                db_path, package, vec_modules, build_p2, enable_async).await?),
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
}

impl AppInstImplInner {
    async fn build(
        db_path: &String,
        package: &MuduPackage,
        vec_modules: Vec<(String, PackageModule)>,
        build_p2:bool,
        enable_async: bool,
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
        initdb(db_path, &app_cfg.name, &sql_text, enable_async).await?;
        Ok(Self {
            package_cfg: app_cfg.clone(),
            enable_async,
            db_path: db_path.clone(),
            schema_mgr,
            modules,
            _conn: Default::default(),
            use_p2: build_p2,
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
    ) -> RS<ProcedureResult> {
        let (procedure, param, new_tx) = self.pre_invoke(task_id, mod_name, proc_name, param).await?;
        let xid = param.session_id();
        let result = if self.use_p2 {
            ProcedureInvokeP2::call(
                &procedure,
                Default::default(),
                param,
            )
        } else {
            ProcedureInvoke1::call(
                &procedure,
                Default::default(),
                param,
            )
        };
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
    ) -> RS<ProcedureResult> {
        if !self.enable_async {
            return Err(m_error!(EC::DBInternalError, "enable async mode when call async procedure"))
        }
        let (procedure, param, new_tx) = self.pre_invoke(task_id, mod_name, proc_name, param).await?;
        let xid = param.session_id();
        let result = ProcedureInvokeP2::call_async(
                &procedure,
                Default::default(),
                param,
        ).await;
        if new_tx {
            if result.is_ok() {
                Context::commit_async(xid).await?;
            } else {
                Context::rollback_async(xid).await?;
            }
        }
        Ok(result?)
    }

    pub fn procedure(&self, mod_name: &str, proc_name: &str) -> Option<Procedure> {
        self.modules.get_sync(mod_name)?.get().procedure(proc_name)
    }

    pub async fn create_conn(&self, task_id: u128) -> RS<()> {
        let db_conn = new_conn(&self.db_path, &self.package_cfg.name, self.enable_async).await?;
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

    async fn pre_invoke(&self,
                  task_id: TaskID,
                  mod_name: &String,
                  proc_name: &String,
                  param: ProcedureParam,
    ) -> RS<(Procedure, ProcedureParam, bool)> {
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
            context.begin_tx().await?;
            (param, true)
        } else {
            (param, false)
        };
        Ok((procedure, param, new_tx))
    }
}

async fn new_conn(db_path: &String, app_name: &String, enable_async: bool) -> RS<DBConn> {
    let db_type = if enable_async {
        "LibSQLAsync".to_string()
    } else {
        "LibSQL".to_string()
    };
    let conn_str = format!("db={} app={} db_type={}", db_path, app_name, db_type);
    let db_conn = DBConnector::connect(&conn_str).await?;
    Ok(db_conn)
}


async fn initdb(db_path: &String, app_name: &String, sql: &String, enable_async: bool) -> RS<()> {
    let init_db_lock = PathBuf::from(&db_path).join(format!("{}.lock", app_name));
    if init_db_lock.exists() {
        return Ok(());
    }
    let conn = new_conn(db_path, app_name, enable_async).await?;
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

#[async_trait]
impl AppInst for AppInstImpl {
    fn cfg(&self) -> &PackageCfg {
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
    ) -> RS<ProcedureResult> {
        self.inner
            .invoke_procedure(task_id, mod_name, proc_name, param).await
    }

    async fn invoke_async(
        &self,
        task_id: TaskID,
        mod_name: &String,
        proc_name: &String,
        param: ProcedureParam,
    ) -> RS<ProcedureResult> {
        self.inner
            .invoke_procedure_async(task_id, mod_name, proc_name, param).await
    }

    fn describe(&self, mod_name: &String, proc_name: &String) -> RS<Arc<ProcDesc>> {
        self.inner.describe_procedure(mod_name, proc_name)
    }
}
