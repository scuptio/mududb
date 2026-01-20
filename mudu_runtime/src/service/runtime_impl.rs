use crate::service::app_inst::AppInst;
use crate::service::runtime::Runtime;
use crate::service::runtime_opt::RuntimeOpt;
use crate::service::runtime_simple::RuntimeSimple;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_utils::notifier::Notifier;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
struct RuntimeImpl {
    inner: Arc<RuntimeSimple>,
}

impl RuntimeImpl {
    pub async fn new(package_path: &String, db_path: &String, rt_opt: RuntimeOpt) -> RS<Self> {
        for ps in [package_path, db_path] {
            let path = PathBuf::from(ps);
            if !path.exists() {
                std::fs::create_dir_all(&path).map_err(|e| {
                    m_error!(EC::IOErr, format!("error creating database directory: {}", ps), e)
                })?
            } else {
                if !path.is_dir() {
                    return Err(m_error!(EC::IOErr, format!("{} is not a directory", ps)));
                }
            }
        }
        let mut runtime = RuntimeSimple::new(package_path, db_path, rt_opt).await?;
        runtime.initialize().await?;
        let ret = Self {
            inner: Arc::new(runtime),
        };
        Ok(ret)
    }
}

#[async_trait]
impl Runtime for RuntimeImpl {
    async fn list(&self) -> Vec<String> {
        self.inner.list()
    }

    async fn app(&self, app_name: String) -> Option<Arc<dyn AppInst>> {
        self.inner.app(app_name)
    }

    async fn install(&self, pkg_path: String) -> RS<()> {
        self.inner.install(pkg_path).await
    }
}

unsafe impl Sync for RuntimeImpl {}

unsafe impl Send for RuntimeImpl {}

pub async fn create_runtime_service(
    package_path: &String,
    db_path: &String,
    opt_initialized_notifier:Option<Notifier>,
    rt_opt: RuntimeOpt
) -> RS<Arc<dyn Runtime>> {
    let runtime = RuntimeImpl::new(package_path, db_path, rt_opt).await?;
    match opt_initialized_notifier {
        Some(notifier) => {
            notifier.notify_all();
        }
        None => {

        }
    }
    Ok(Arc::new(runtime))
}
