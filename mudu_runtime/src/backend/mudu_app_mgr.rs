use crate::backend::app_mgr::AppMgr;
use crate::backend::mududb_cfg::MuduDBCfg;
use crate::service::app_list::{AppList, AppListItem};
use crate::service::mudu_package::MuduPackage;
use crate::service::runtime::Runtime;
use crate::service::runtime_impl::create_runtime_service;
use crate::service::runtime_opt::RuntimeOpt;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_binding::procedure::procedure_invoke;
use mudu_kernel::server_ur::procedure_runtime::ProcInvoker;
use mudu_kernel::server_ur::worker_local::WorkerLocalRef;
use std::collections::HashSet;
use std::env::temp_dir;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, Weak};

const MPK_EXTENSION: &str = "mpk";

struct MuduProcInvoker {
    cfg: MuduDBCfg,
    runtime: RwLock<Arc<dyn Runtime>>,
    enable_async: bool,
}

impl MuduProcInvoker {
    fn new(cfg: MuduDBCfg, runtime: Arc<dyn Runtime>, enable_async: bool) -> Self {
        Self {
            cfg,
            runtime: RwLock::new(runtime),
            enable_async,
        }
    }

    async fn install(&self, pkg_path: String) -> RS<()> {
        let runtime = self.runtime.read().unwrap().clone();
        runtime.install(pkg_path).await
    }

    async fn reload(&self) -> RS<()> {
        let runtime = create_runtime_from_cfg(&self.cfg).await?;
        *self.runtime.write().unwrap() = runtime;
        Ok(())
    }
}

#[async_trait]
impl ProcInvoker for MuduProcInvoker {
    async fn invoke(
        &self,
        session_id: OID,
        procedure_name: &str,
        procedure_parameters: Vec<u8>,
        worker_local: WorkerLocalRef,
    ) -> RS<Vec<u8>> {
        let (app_name, mod_name, proc_name) = parse_procedure_name(procedure_name)?;
        let runtime = self.runtime.read().unwrap().clone();
        let app = runtime.app(app_name.clone()).await.ok_or_else(|| {
            m_error!(
                EC::NoSuchElement,
                format!("no such application for procedure invoke: {}", app_name)
            )
        })?;

        let task_id = app.task_create().await?;
        let invoke_result = async {
            let mut param = procedure_invoke::deserialize_param(&procedure_parameters)?;
            param.set_session_id(session_id);
            let result = if self.enable_async {
                app.invoke_async(task_id, &mod_name, &proc_name, param, Some(worker_local))
                    .await?
            } else {
                app.invoke(task_id, &mod_name, &proc_name, param, Some(worker_local))
                    .await?
            };
            procedure_invoke::serialize_result(Ok(result))
        }
        .await;

        let task_end_result = app.task_end(task_id);
        match (invoke_result, task_end_result) {
            (Ok(result), Ok(())) => Ok(result),
            (Err(invoke_err), _) => Err(invoke_err),
            (Ok(_), Err(task_end_err)) => Err(task_end_err),
        }
    }
}

pub struct ListOption {
    /// Optional application-name filter.
    ///
    /// When this list is empty, the implementation must return all
    /// applications visible to the manager. When it is non-empty, the
    /// implementation must only return the named applications that currently
    /// exist.
    pub names: Vec<String>,
}

impl Default for ListOption {
    fn default() -> Self {
        Self { names: Vec::new() }
    }
}

pub struct MuduAppMgr {
    cfg: MuduDBCfg,
    created_invokers: Mutex<Vec<Weak<MuduProcInvoker>>>,
}

impl MuduAppMgr {
    pub fn new(cfg: MuduDBCfg) -> Self {
        Self {
            cfg,
            created_invokers: Mutex::new(Vec::new()),
        }
    }

    fn register_invoker(&self, invoker: &Arc<MuduProcInvoker>) {
        let mut created_invokers = self.created_invokers.lock().unwrap();
        created_invokers.push(Arc::downgrade(invoker));
    }

    fn live_invokers(&self) -> Vec<Arc<MuduProcInvoker>> {
        let mut created_invokers = self.created_invokers.lock().unwrap();
        let mut live = Vec::with_capacity(created_invokers.len());
        created_invokers.retain(|weak| match weak.upgrade() {
            Some(invoker) => {
                live.push(invoker);
                true
            }
            None => false,
        });
        live
    }
}

#[async_trait(?Send)]
impl AppMgr for MuduAppMgr {
    async fn install(&self, mpk_binary: Vec<u8>) -> RS<()> {
        let mpk_path = self.cfg.mpk_path.clone();

        fs::create_dir_all(&mpk_path)
            .map_err(|e| m_error!(EC::IOErr, "create mpk directory error", e))?;
        let temp_path = temp_package_path(&temp_dir().to_string_lossy());
        fs::write(&temp_path, &mpk_binary)
            .map_err(|e| m_error!(EC::IOErr, "write temp mpk file error", e))?;
        let package = MuduPackage::load(&temp_path)?;
        let final_path = PathBuf::from(&mpk_path).join(format!("{}.mpk", package.package_cfg.name));
        fs::write(&final_path, &mpk_binary)
            .map_err(|e| m_error!(EC::IOErr, "write final mpk file error", e))?;

        let install_path = final_path
            .to_str()
            .ok_or_else(|| m_error!(EC::IOErr, "temp package path is not valid utf-8"))?
            .to_string();
        for invoker in self.live_invokers() {
            invoker.install(install_path.clone()).await?;
        }
        Ok(())
    }

    async fn uninstall(&self, app_name: Vec<u8>) -> RS<()> {
        let app_name = String::from_utf8(app_name)
            .map_err(|e| m_error!(EC::DecodeErr, "decode app name error", e))?;
        let package_path = find_package_path_by_app_name(&self.cfg.mpk_path, &app_name)?
            .ok_or_else(|| m_error!(EC::NoSuchElement, format!("no such app {}", app_name)))?;
        fs::remove_file(&package_path)
            .map_err(|e| m_error!(EC::IOErr, "remove app package error", e))?;
        for invoker in self.live_invokers() {
            invoker.reload().await?;
        }
        Ok(())
    }

    async fn list(&self, option: &ListOption) -> RS<AppList> {
        let names = option.names.iter().cloned().collect::<HashSet<String>>();
        let mut apps = load_packages(&self.cfg.mpk_path)?
            .into_iter()
            .filter(|package| names.is_empty() || names.contains(&package.package_cfg.name))
            .map(|package| AppListItem {
                info: package.package_cfg,
                ddl: package.ddl_sql,
                mod_proc_desc: package.package_desc,
            })
            .collect::<Vec<_>>();
        apps.sort_by(|a, b| a.info.name.cmp(&b.info.name));
        Ok(AppList { apps })
    }

    async fn create_invoker(&self, cfg: &MuduDBCfg) -> RS<Arc<dyn ProcInvoker>> {
        let cfg = cfg.clone();
        let invoker = build_owned_proc_invoker(&cfg).await?;
        self.register_invoker(&invoker);
        Ok(invoker as Arc<dyn ProcInvoker>)
    }
}

async fn create_runtime_from_cfg(cfg: &MuduDBCfg) -> RS<Arc<dyn Runtime>> {
    let runtime_target = cfg.runtime_target();
    let enable_async = cfg.enable_async && runtime_target.uses_component_model();
    create_runtime_service(
        &cfg.mpk_path,
        &cfg.data_path,
        None,
        RuntimeOpt {
            target: runtime_target,
            enable_async,
        },
    )
    .await
}

async fn build_owned_proc_invoker(cfg: &MuduDBCfg) -> RS<Arc<MuduProcInvoker>> {
    let runtime = create_runtime_from_cfg(cfg).await?;
    let enable_async = cfg.enable_async && cfg.runtime_target().uses_component_model();
    Ok(Arc::new(MuduProcInvoker::new(
        cfg.clone(),
        runtime,
        enable_async,
    )))
}

fn load_packages<P: AsRef<Path>>(mpk_path: P) -> RS<Vec<MuduPackage>> {
    let mut packages = Vec::new();
    let path = mpk_path.as_ref();
    if !path.exists() {
        return Ok(packages);
    }
    for entry in
        fs::read_dir(path).map_err(|e| m_error!(EC::IOErr, "read mpk directory error", e))?
    {
        let entry = entry.map_err(|e| m_error!(EC::IOErr, "read mpk directory entry error", e))?;
        let path = entry.path();
        if is_mpk_file(&path) {
            packages.push(MuduPackage::load(&path)?);
        }
    }
    Ok(packages)
}

fn find_package_path_by_app_name<P: AsRef<Path>>(
    mpk_path: P,
    app_name: &str,
) -> RS<Option<PathBuf>> {
    let path = mpk_path.as_ref();
    if !path.exists() {
        return Ok(None);
    }
    for entry in
        fs::read_dir(path).map_err(|e| m_error!(EC::IOErr, "read mpk directory error", e))?
    {
        let entry = entry.map_err(|e| m_error!(EC::IOErr, "read mpk directory entry error", e))?;
        let path = entry.path();
        if !is_mpk_file(&path) {
            continue;
        }
        let package = MuduPackage::load(&path)?;
        if package.package_cfg.name == app_name {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn is_mpk_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .map(|ext| ext.to_ascii_lowercase() == MPK_EXTENSION)
            .unwrap_or(false)
}

fn temp_package_path(base_dir: &str) -> PathBuf {
    PathBuf::from(base_dir).join(format!("tmp_install_{:x}.mpk", mudu::common::id::gen_oid()))
}

fn parse_procedure_name(procedure_name: &str) -> RS<(String, String, String)> {
    let mut segments = procedure_name.split('/');
    let app_name = segments.next().unwrap_or_default();
    let mod_name = segments.next().unwrap_or_default();
    let proc_name = segments.next().unwrap_or_default();
    if app_name.is_empty()
        || mod_name.is_empty()
        || proc_name.is_empty()
        || segments.next().is_some()
    {
        return Err(m_error!(
            EC::ParseErr,
            format!(
                "invalid procedure name '{}', expected app/module/procedure",
                procedure_name
            )
        ));
    }
    Ok((
        app_name.to_string(),
        mod_name.to_string(),
        proc_name.to_string(),
    ))
}
