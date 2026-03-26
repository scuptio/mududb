use crate::service::mudu_package::MuduPackage;
use crate::service::package_module::PackageModule;
use crate::service::runtime_opt::RuntimeOpt;
use crate::service::wt_runtime_component::WTRuntimeComponent;
use crate::service::wt_runtime_p1::WTRuntimeP1;
use mudu::common::result::RS;

pub struct WTRuntime {
    inner: WTRuntimeKind,
}

impl WTRuntime {
    pub fn build_p1() -> RS<Self> {
        Ok(Self {
            inner: WTRuntimeKind::build_p1()?,
        })
    }

    pub fn build_component(runtime_opt: &RuntimeOpt) -> RS<Self> {
        Ok(Self {
            inner: WTRuntimeKind::build_component(runtime_opt)?,
        })
    }

    pub fn instantiate(&mut self) -> RS<()> {
        self.inner.instantiate()
    }

    pub fn compile_modules(&self, package: &MuduPackage) -> RS<Vec<(String, PackageModule)>> {
        self.inner.compile_modules(package)
    }
}

enum WTRuntimeKind {
    P1(WTRuntimeP1),
    Component(WTRuntimeComponent),
}

impl WTRuntimeKind {
    fn build_p1() -> RS<Self> {
        Ok(Self::P1(WTRuntimeP1::build()?))
    }

    fn build_component(runtime_opt: &RuntimeOpt) -> RS<Self> {
        Ok(Self::Component(WTRuntimeComponent::build(runtime_opt)?))
    }

    fn instantiate(&mut self) -> RS<()> {
        match self {
            WTRuntimeKind::P1(r) => r.instantiate(),
            WTRuntimeKind::Component(r) => r.instantiate(),
        }
    }

    fn compile_modules(&self, package: &MuduPackage) -> RS<Vec<(String, PackageModule)>> {
        match self {
            WTRuntimeKind::P1(r) => r.compile_modules(package),
            WTRuntimeKind::Component(r) => r.compile_modules(package),
        }
    }
}
