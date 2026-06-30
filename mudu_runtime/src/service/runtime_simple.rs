use crate::service::app_inst::AppInst;
use crate::service::app_inst_impl::AppInstImpl;
use crate::service::file_name;
use crate::service::mudu_package::MuduPackage;
use crate::service::runtime_opt::RuntimeOpt;
use crate::service::wt_runtime::WTRuntime;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::fs;
use scc::HashMap as SCCHashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct RuntimeSimple {
    rt_opt: RuntimeOpt,
    db_path: String,
    package_path: String,
    wt_runtime: WTRuntime,
    apps: SCCHashMap<String, AppInstImpl>,
}

async fn load_package_files<P1, F>(package_dir_path: P1, handle_package_file: F) -> RS<()>
where
    P1: AsRef<Path>,
    F: AsyncFn(String) -> RS<()>,
{
    let dir = package_dir_path.as_ref();
    for entry in fs::sync::sync_read_dir_entries(dir)? {
        let path = entry.path();

        // check file name
        if path.is_file()
            && let Some(ext) = path.extension()
            && ext.to_ascii_lowercase() == file_name::APP_PACKAGE_EXTENSION
        {
            let path_str = path
                .as_path()
                .to_str()
                .ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::InvalidUtf8,
                        format!("path {:?} to str error", path)
                    )
                })?
                .to_string();
            handle_package_file(path_str).await?;
        }
    }

    Ok(())
}

fn load_package_from_file<P: AsRef<Path>>(path_ref: P) -> RS<MuduPackage> {
    let path_buf = PathBuf::from(path_ref.as_ref());
    if !path_buf.is_file() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("path {} is not a file", path_ref.as_ref().to_string_lossy())
        ));
    }
    if let Some(ext) = path_buf.extension() {
        if ext.to_ascii_lowercase() == file_name::APP_PACKAGE_EXTENSION {
            let app_package = MuduPackage::load(&path_buf)?;
            Ok(app_package)
        } else {
            Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "package {} must be with {} extension",
                    path_ref.as_ref().to_string_lossy(),
                    file_name::APP_PACKAGE_EXTENSION
                )
            ))
        }
    } else {
        Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!(
                "package {} must be with {} extension",
                path_ref.as_ref().to_string_lossy(),
                file_name::APP_PACKAGE_EXTENSION
            )
        ))
    }
}
impl RuntimeSimple {
    pub async fn new(package_path: &str, db_path: &str, rt_opt: RuntimeOpt) -> RS<RuntimeSimple> {
        let wt_runtime = WTRuntime::build_component(&rt_opt)?;
        Ok(Self {
            rt_opt,
            package_path: package_path.to_owned(),
            db_path: db_path.to_owned(),
            wt_runtime,
            apps: Default::default(),
        })
    }

    pub async fn initialize(&mut self) -> RS<()> {
        self.wt_runtime.instantiate()?;
        if !fs::sync::sync_path_exists(&self.db_path) {
            fs::sync::sync_create_dir_all(&self.db_path)?
        } else if let metadata = fs::sync::sync_metadata(&self.db_path)?
            && metadata.is_file()
        {
            return Err(mudu_error!(
                ErrorCode::NotADirectory,
                format!("path {} is a not a directory", self.db_path)
            ));
        }

        load_package_files(&self.package_path, async |path| {
            self.init_mpk(path).await?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    async fn init_mpk<P: AsRef<Path>>(&self, path: P) -> RS<String> {
        let app_package = load_package_from_file(path.as_ref())?;
        let modules = self.wt_runtime.compile_modules(&app_package)?;
        let app_instance = AppInstImpl::build(
            &self.db_path,
            &app_package,
            modules,
            self.rt_opt.component_target(),
            self.rt_opt.enable_async,
            self.rt_opt.sever_mode,
            self.rt_opt.async_runtime(),
        )
        .await?;
        let mpk_name = app_instance.name().clone();
        let _ = self
            .apps
            .insert_sync(app_instance.name().to_string(), app_instance);
        Ok(mpk_name)
    }

    async fn install_pkg<P: AsRef<Path>>(&self, path: P) -> RS<()> {
        let mpk_name = self.init_mpk(path.as_ref().to_path_buf()).await?;
        let pkg_path = PathBuf::from(self.package_path.clone());
        let parent = path.as_ref().parent().ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidArgument,
                "package path has no parent directory"
            )
        })?;
        if parent.eq(&pkg_path) {
            return Ok(());
        }
        let output = PathBuf::from(&self.package_path).join(format!("{}.mpk", mpk_name));
        fs::sync::sync_copy(&path, &output)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<String> {
        let mut vec = Vec::new();
        let _ = self.apps.iter_sync(|k, _v| {
            vec.push(k.to_string());
            true
        });
        vec
    }

    pub fn app(&self, name: String) -> Option<Arc<dyn AppInst>> {
        self.apps
            .get_sync(&name)
            .map(|e| Arc::new(e.get().clone()) as Arc<dyn AppInst>)
    }

    pub async fn install(&self, pkg_path: String) -> RS<()> {
        self.install_pkg(pkg_path).await?;
        Ok(())
    }

    pub fn async_runtime(&self) -> Option<Arc<dyn AsyncIoProvider>> {
        self.rt_opt.async_runtime()
    }
}
