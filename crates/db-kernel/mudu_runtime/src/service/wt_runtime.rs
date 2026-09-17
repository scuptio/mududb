use crate::service::app_package::AppPackage;
use crate::service::package_module::PackageModule;
use crate::service::runtime_opt::RuntimeOpt;
use crate::service::wt_runtime_component::WTRuntimeComponent;
use mudu::common::result::RS;

pub struct WTRuntime {
    inner: WTRuntimeComponent,
}

impl WTRuntime {
    pub fn build_component(runtime_opt: &RuntimeOpt) -> RS<Self> {
        Ok(Self {
            inner: WTRuntimeComponent::build(runtime_opt)?,
        })
    }

    pub fn instantiate(&mut self) -> RS<()> {
        self.inner.instantiate()
    }

    pub fn compile_modules(&self, package: &AppPackage) -> RS<Vec<(String, PackageModule)>> {
        self.inner.compile_modules(package)
    }
}
