use crate::backend::app_mgr::AppMgr;
use crate::backend::mudud_cfg::MuduDBCfg;
use crate::service::app_list::{AppList, AppListItem};
use crate::service::app_package::AppPackage;
use crate::service::runtime::Runtime;
use crate::service::runtime_impl::create_runtime_service;
use crate::service::runtime_opt::RuntimeOpt;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::xid::INVALID_OID;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::procedure::procedure_invoke;
use mudu_kernel::server::async_func_runtime::AsyncFuncInvoker;
use mudu_kernel::server::worker_local::WorkerLocalRef;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::sync::SMutex;
use mudu_sys::sync::SRwLock;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

const MPK_EXTENSION: &str = "mpk";

struct MuduProcInvoker {
    cfg: MuduDBCfg,
    runtime: SRwLock<Arc<dyn Runtime>>,
    enable_async: bool,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
}

impl MuduProcInvoker {
    fn new(
        cfg: MuduDBCfg,
        runtime: Arc<dyn Runtime>,
        enable_async: bool,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    ) -> Self {
        Self {
            cfg,
            runtime: SRwLock::new(runtime),
            enable_async,
            async_runtime,
        }
    }

    async fn install(&self, pkg_path: String) -> RS<()> {
        let runtime = self.runtime.read()?.clone();
        runtime.install(pkg_path).await
    }

    async fn reload(&self) -> RS<()> {
        let runtime = create_runtime_from_cfg(&self.cfg, self.async_runtime.clone()).await?;
        *self.runtime.write()? = runtime;
        Ok(())
    }
}

#[async_trait]
impl AsyncFuncInvoker for MuduProcInvoker {
    async fn invoke(
        &self,
        session_id: OID,
        procedure_name: &str,
        procedure_parameters: Vec<u8>,
        worker_local: WorkerLocalRef,
    ) -> RS<Vec<u8>> {
        let (app_name, mod_name, proc_name) = parse_procedure_name(procedure_name)?;
        let runtime = self.runtime.read()?.clone();
        let app = runtime.app(app_name.clone()).await.ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such application for procedure invoke: {}", app_name)
            )
        })?;

        let task_id = app.task_create().await?;
        let invoke_result = async {
            let mut param = procedure_invoke::deserialize_param(&procedure_parameters)?;
            let _ = session_id;
            // TCP session ids belong to the transport/session manager. Procedure
            // host syscalls require a database Context xid, so let the app
            // runtime create one per invocation.
            param.set_session_id(INVALID_OID);
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

/// Options for listing applications.
#[derive(Default)]
pub struct ListOption {
    /// Optional application-name filter.
    ///
    /// When this list is empty, the implementation must return all
    /// applications visible to the manager. When it is non-empty, the
    /// implementation must only return the named applications that currently
    /// exist.
    pub names: Vec<String>,
}

/// MuduDB application manager implementation.
pub struct MuduAppMgr {
    cfg: MuduDBCfg,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    created_invokers: SMutex<Vec<Weak<MuduProcInvoker>>>,
}

impl MuduAppMgr {
    /// Creates a new application manager from configuration.
    pub fn new(cfg: MuduDBCfg) -> Self {
        Self::new_with_async_runtime(cfg, None)
    }

    /// Creates a new application manager with an optional async runtime.
    pub fn new_with_async_runtime(
        cfg: MuduDBCfg,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    ) -> Self {
        Self {
            cfg,
            async_runtime,
            created_invokers: SMutex::new(Vec::new()),
        }
    }

    fn register_invoker(&self, invoker: &Arc<MuduProcInvoker>) -> RS<()> {
        let mut created_invokers = self.created_invokers.lock()?;
        created_invokers.push(Arc::downgrade(invoker));
        Ok(())
    }

    fn live_invokers(&self) -> RS<Vec<Arc<MuduProcInvoker>>> {
        let mut created_invokers = self.created_invokers.lock()?;
        let mut live = Vec::with_capacity(created_invokers.len());
        created_invokers.retain(|weak| match weak.upgrade() {
            Some(invoker) => {
                live.push(invoker);
                true
            }
            None => false,
        });
        Ok(live)
    }
}

#[async_trait(?Send)]
impl AppMgr for MuduAppMgr {
    async fn install(&self, mpk_binary: Vec<u8>) -> RS<()> {
        let mpk_path = self.cfg.mpk_path.clone();

        // The install handler performs synchronous file I/O and package parsing.
        // Run it on actix's blocking thread pool so the single async worker thread
        // stays responsive, especially under heavy instrumentation such as ASan.
        let final_path =
            actix_web::web::block(move || write_package_to_disk(&mpk_path, &mpk_binary))
                .await
                .map_err(|e| mudu_error!(ErrorCode::Thread, "blocking install task failed", e))??;

        let install_path = final_path
            .to_str()
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidUtf8,
                    "temp package path is not valid utf-8"
                )
            })?
            .to_string();
        for invoker in self.live_invokers()? {
            invoker.install(install_path.clone()).await?;
        }
        Ok(())
    }

    async fn uninstall(&self, app_name: Vec<u8>) -> RS<()> {
        let app_name = String::from_utf8(app_name)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode app name error", e))?;
        let package_path = find_package_path_by_app_name(&self.cfg.mpk_path, &app_name)?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("no such app {}", app_name)
                )
            })?;
        mudu_sys::fs::sync::remove_file(&package_path)?;
        for invoker in self.live_invokers()? {
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

    async fn create_invoker(&self, cfg: &MuduDBCfg) -> RS<Arc<dyn AsyncFuncInvoker>> {
        let cfg = cfg.clone();
        let invoker = build_owned_proc_invoker(&cfg, self.async_runtime.clone()).await?;
        self.register_invoker(&invoker)?;
        Ok(invoker as Arc<dyn AsyncFuncInvoker>)
    }
}

async fn create_runtime_from_cfg(
    cfg: &MuduDBCfg,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
) -> RS<Arc<dyn Runtime>> {
    let component_target = cfg.component_target();
    let enable_async = cfg.enable_async;
    create_runtime_service(
        &cfg.mpk_path,
        &cfg.db_path,
        None,
        RuntimeOpt {
            component_target,
            enable_async,
            sever_mode: cfg.server_mode,
            async_runtime: async_runtime
                .or_else(|| RuntimeOpt::build_async_runtime(cfg.server_mode)),
        },
    )
    .await
}

async fn build_owned_proc_invoker(
    cfg: &MuduDBCfg,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
) -> RS<Arc<MuduProcInvoker>> {
    let runtime = create_runtime_from_cfg(cfg, async_runtime.clone()).await?;
    let enable_async = cfg.enable_async;
    Ok(Arc::new(MuduProcInvoker::new(
        cfg.clone(),
        runtime,
        enable_async,
        async_runtime,
    )))
}

fn write_package_to_disk(mpk_path: &str, mpk_binary: &[u8]) -> RS<PathBuf> {
    mudu_sys::fs::sync::create_dir_all(mpk_path)?;
    let temp_path = temp_package_path(&mudu_sys::env_var::temp_dir().to_string_lossy());
    mudu_sys::fs::sync::write(&temp_path, mpk_binary)?;
    let package = AppPackage::load(&temp_path)?;
    let final_path = PathBuf::from(mpk_path).join(format!("{}.mpk", package.package_cfg.name));
    mudu_sys::fs::sync::write(&final_path, mpk_binary)?;
    Ok(final_path)
}

fn load_packages<P: AsRef<Path>>(mpk_path: P) -> RS<Vec<AppPackage>> {
    let mut packages = Vec::new();
    let path = mpk_path.as_ref();
    if !mudu_sys::fs::sync::path_exists(path) {
        return Ok(packages);
    }
    for entry in mudu_sys::fs::sync::read_dir_entries(path)? {
        let path = entry.path();
        if is_mpk_file(&path) {
            packages.push(AppPackage::load(&path)?);
        }
    }
    Ok(packages)
}

fn find_package_path_by_app_name<P: AsRef<Path>>(
    mpk_path: P,
    app_name: &str,
) -> RS<Option<PathBuf>> {
    let path = mpk_path.as_ref();
    if !mudu_sys::fs::sync::path_exists(path) {
        return Ok(None);
    }
    for entry in mudu_sys::fs::sync::read_dir_entries(path)? {
        let path = entry.path();
        if !is_mpk_file(&path) {
            continue;
        }
        let package = AppPackage::load(&path)?;
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
    PathBuf::from(base_dir).join(format!("tmp_install_{:x}.mpk", mudu_utils::oid::gen_oid()))
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
        return Err(mudu_error!(
            ErrorCode::Parse,
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
