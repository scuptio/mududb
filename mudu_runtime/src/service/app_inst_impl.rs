use crate::db_connector::DBConnector;
use crate::procedure::procedure::Procedure;
use crate::procedure::wasi_context::WasiContext;
use crate::resolver::schema_mgr::SchemaMgr;
use crate::service::app_inst::AppInst;
use crate::service::app_module::AppModule;
use crate::service::app_package::AppPackage;
use crate::service::procedure_invoke::ProcedureInvoke;
use mudu::common::app_cfg::AppCfg;
use mudu::common::result::RS;
use mudu::common::xid::is_xid_invalid;
use mudu::database::db_conn::DBConn;
use mudu::database::sql::Context;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::procedure::proc_desc::ProcDesc;
use mudu::procedure::proc_param::ProcParam;
use mudu::procedure::proc_result::ProcResult;
use mudu::utils::app_proc_desc::AppProcDesc;
use mudu_utils::task_id::{new_task_id, TaskID};
use scc::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use wasmtime::{Engine, Linker, Module};
use wasmtime_wasi::WasiCtxBuilder;

#[derive(Clone)]
pub struct AppInstImpl {
    inner: Arc<AppInstImplInner>,
}

struct AppInstImplInner {
    app_cfg: AppCfg,
    db_path: String,
    schema_mgr: SchemaMgr,
    modules: HashMap<String, AppModule>,
    _conn: HashMap<u128, Arc<dyn DBConn>>,
}

impl AppInstImpl {
    pub fn build(
        engine: &Engine,
        linker: &Linker<WasiContext>,
        db_path: &String,
        package: AppPackage,
    ) -> RS<Self> {
        Ok(Self {
            inner: Arc::new(AppInstImplInner::build(engine, linker, db_path, package)?),
        })
    }

    pub fn connection(&self, task_id: u128) -> Option<Arc<dyn DBConn>> {
        self.inner.connection(task_id)
    }

    pub fn create_conn(&self, task_id: u128) -> RS<()> {
        self.inner.create_conn(task_id)
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
    fn build(
        engine: &Engine,
        linker: &Linker<WasiContext>,
        db_path: &String,
        package: AppPackage,
    ) -> RS<Self> {
        let mut package = package;
        let modules = HashMap::new();
        let app_cfg = package.app_cfg;
        let ddl_sql = package.ddl_sql;
        let init_sql = package.initdb_sql;
        let schema_mgr = SchemaMgr::from_sql_text(&ddl_sql)?;
        let app_proc_desc: AppProcDesc = package.app_proc_desc;
        for (mod_name, vec_desc) in app_proc_desc.modules {
            let byte_code = package.modules.remove(&mod_name).ok_or_else(|| m_error!(
                EC::NoneErr,
                format!("no such module named {}", mod_name)
            ))?;
            let module =
                Self::build_app_module(engine, linker, mod_name.clone(), byte_code, vec_desc)?;
            let _ = modules.insert_sync(mod_name, module);
        }
        SchemaMgr::add_mgr(app_cfg.name.clone(), schema_mgr.clone());
        let sql_text = ddl_sql + init_sql.as_str();
        initdb(db_path, &app_cfg.name, &sql_text)?;
        Ok(Self {
            app_cfg,
            db_path: db_path.clone(),
            schema_mgr,
            modules,
            _conn: Default::default(),
        })
    }

    fn build_app_module(
        engine: &Engine,
        linker: &Linker<WasiContext>,
        name: String,
        byte_code: Vec<u8>,
        desc_vec: Vec<ProcDesc>,
    ) -> RS<AppModule> {
        let module = Module::from_binary(&engine, &byte_code).map_err(|e| {
            m_error!(
                EC::MuduError,
                format!("build module {} from binary error", name),
                e
            )
        })?;

        let instance_pre = linker.instantiate_pre(&module).map_err(|e| {
            m_error!(
                EC::MuduError,
                format!("instantiate module {} error", name),
                e
            )
        })?;
        AppModule::new(instance_pre, desc_vec)
    }

    fn build_context() -> WasiContext {
        let wasi = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_args()
            .build_p1();
        let context = WasiContext::new(wasi);
        context
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
        let procedure = self.procedure(mod_name, proc_name).ok_or_else(|| m_error!(
            EC::NoneErr,
            format!("no such module named {} {}", mod_name, proc_name)
        ))?;
        Ok(procedure.desc())
    }

    pub fn invoke_procedure(
        &self,
        task_id:TaskID,
        mod_name: &String,
        proc_name: &String,
        param: ProcParam,
    ) -> RS<ProcResult> {
        let procedure = self.procedure(mod_name, proc_name)
            .ok_or_else(|| {
                    m_error!(EC::NoneErr,format!("procedure {}/{} not found", mod_name, proc_name))
            })?;

        let existing_xid = param.xid();
        let param = if is_xid_invalid(&existing_xid) {
            let conn = self.connection(task_id).ok_or_else(||m_error!(
                EC::NoneErr,
                format!("no such task named {}", task_id)
            ))?;
            let context = Context::create(conn)?;
            let mut param = param;
            param.set_xid(context.xid());
            param
        } else {
            param
        };
        let xid = param.xid();
        let invoke_name = format!(
            "{}{}",
            mudu::procedure::proc::MUDU_PROC_PREFIX,
            procedure.proc_name()
        );
        let result = ProcedureInvoke::call(
            Self::build_context(),
            procedure.instance(),
            Default::default(),
            invoke_name,
            param,
        );
        if is_xid_invalid(&existing_xid) {
            if result.is_ok() {
                Context::commit(xid)?;
            } else {
                Context::rollback(xid)?;
            }
        }
        Ok(result?)
    }

    pub fn procedure(&self, mod_name: &str, proc_name: &str) -> Option<Procedure> {
        self.modules.get_sync(mod_name)?.get().procedure(proc_name)
    }

    pub fn create_conn(&self, task_id: u128) -> RS<()> {
        let db_conn = new_conn(&self.db_path, &self.app_cfg.name)?;
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
    pub fn connection(&self, task_id: u128) -> Option<Arc<dyn DBConn>> {
        self._conn.get_sync(&task_id).map(|conn| conn.clone())
    }

    pub fn name(&self) -> &String {
        &self.app_cfg.name
    }

    pub fn schema_mgr(&self) -> &SchemaMgr {
        &self.schema_mgr
    }
}

fn new_conn(db_path: &String, app_name: &String) -> RS<Arc<dyn DBConn>> {
    let conn_str = format!("db={} app={} db_type=LibSQL", db_path, app_name);
    let db_conn = DBConnector::connect(&conn_str)?;
    Ok(db_conn)
}

fn initdb(db_path: &String, app_name: &String, sql: &String) -> RS<()> {
    let init_db_lock = PathBuf::from(&db_path).join(format!("{}.lock", app_name));
    if init_db_lock.exists() {
        return Ok(());
    }
    let conn = new_conn(db_path, app_name)?;
    conn.exec_sql(sql)?;
    File::create(&init_db_lock).map_err(|e| {
        m_error!(
            EC::IOErr,
            format!("failed to create file: {}", init_db_lock.to_str().unwrap()),
            e
        )
    })?;
    Ok(())
}

impl AppInst for AppInstImpl {
    fn task_create(&self) -> RS<TaskID> {
        let id = new_task_id();
        self.create_conn(id)?;
        Ok(id)
    }

    fn task_end(&self, task_id: TaskID) -> RS<()> {
        self.remove_conn(task_id)
    }

    fn procedure(&self) -> RS<Vec<(String, String)>> {
        self.inner.list_procedure()
    }

    fn invoke(&self, task_id:TaskID, mod_name: &String, proc_name: &String, param: ProcParam) -> RS<ProcResult> {
        self.inner.invoke_procedure(task_id, mod_name, proc_name, param)
    }

    fn describe(&self, mod_name: &String, proc_name: &String) -> RS<Arc<ProcDesc>> {
        self.inner.describe_procedure(mod_name, proc_name)
    }
}
