use super::invocation_epoch::EpochClock;
use crate::service::app_package::AppPackage;
use crate::service::package_module::PackageModule;
use crate::service::runtime_opt::{ComponentTarget, RuntimeOpt};
use crate::service::wasi_context_component;
use crate::service::wasi_context_component::WasiContextComponent;
use crate::service::wt_instance_pre::WTInstancePre;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use mudu_contract::procedure::proc_desc::ProcDesc;
use std::sync::Arc;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Module};
use wasmtime_wasi::p2::add_to_linker_sync;

/// Wasmtime component-model runtime: the engine plus the linker the Mudu
/// host functions and WASI are registered with.
pub struct WTRuntimeComponent {
    runtime_opt: RuntimeOpt,
    engine: Engine,
    clock: Arc<EpochClock>,
    linker: Linker<WasiContextComponent>,
}

impl WTRuntimeComponent {
    /// Creates the engine and linker from the runtime options, enabling the
    /// component-model async features when `enable_async` is set.
    pub fn build(runtime_opt: &RuntimeOpt) -> RS<Self> {
        let runtime_opt = runtime_opt.clone();
        let mut cfg = Config::new();
        cfg.wasm_component_model(true);
        cfg.consume_fuel(true)
            .epoch_interruption(true)
            .wasm_threads(false);
        if runtime_opt.enable_async {
            cfg.wasm_component_model_async(true)
                .wasm_component_model_more_async_builtins(true);
        }
        let engine = Engine::new(&cfg).map_err(|e| {
            mudu_error!(
                ErrorCode::Internal,
                "failed create new wasm runtime engine",
                e
            )
        })?;
        // Configure linker with host functions
        let linker = Linker::new(&engine);
        let clock = EpochClock::start(engine.clone())?;
        Ok(Self {
            clock,
            runtime_opt,
            engine,
            linker,
        })
    }

    /// Registers the Mudu sync/async host functions and the WASI
    /// implementation with the linker.
    pub fn instantiate(&mut self) -> RS<()> {
        let component_target = self.runtime_opt.component_target();
        wasi_context_component::async_host::mududb::async_api::system::add_to_linker::<_, HasSelf<_>>(
            &mut self.linker,
            |c| c,
        )
        .map_err(|e| mudu_error!(ErrorCode::Internal, "instantiate, link async function error", e))?;
        wasi_context_component::sync_host::mududb::api::system::add_to_linker::<_, HasSelf<_>>(
            &mut self.linker,
            |c| c,
        )
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Internal,
                "instantiate, link sync function error",
                e
            )
        })?;
        match component_target {
            ComponentTarget::P2 => add_to_linker_sync(&mut self.linker).map_err(|e| {
                mudu_error!(
                    ErrorCode::Internal,
                    "wasmtime_wasi add_to_linker_sync error",
                    e
                )
            })?,
            ComponentTarget::P3 => {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    "component target p3 is not implemented yet"
                ));
            }
        }
        Ok(())
    }

    /// Compiles and pre-instantiates every module of a package.
    pub fn compile_modules(&self, package: &AppPackage) -> RS<Vec<(String, PackageModule)>> {
        let modules =
            instantiate_component_modules(&self.engine, &self.linker, package, &self.clock)?;
        Ok(modules)
    }
}

fn instantiate_component(
    clock: &Arc<EpochClock>,
    engine: &Engine,
    linker: &Linker<WasiContextComponent>,
    name: String,
    byte_code: &[u8],
    desc_vec: &[ProcDesc],
) -> RS<PackageModule> {
    let component = match Component::from_binary(engine, byte_code) {
        Ok(component) => component,
        Err(component_err) => {
            if Module::from_binary(engine, byte_code).is_ok() {
                return Err(mudu_error!(
                    ErrorCode::DomainViolation,
                    format!(
                        "package module {} is a WebAssembly module, but runtime target is component; rebuild the package for wasm32-wasip2",
                        name
                    ),
                    component_err
                ));
            }
            return Err(mudu_error!(
                ErrorCode::DomainViolation,
                format!("build component {} from binary error", name),
                component_err
            ));
        }
    };

    let instance_pre = linker.instantiate_pre(&component).map_err(|e| {
        mudu_error!(
            ErrorCode::DomainViolation,
            format!("instantiate module {} error", name),
            e
        )
    })?;

    // Components importing the async host API (`mududb:async-api/*`) get
    // async-required stores from wasmtime; remember that so the invoke path
    // can pick sync or async entry points per component.
    let requires_async = component
        .component_type()
        .imports(engine)
        .any(|(import_name, _)| import_name.starts_with("mududb:async-api"));

    PackageModule::new(
        WTInstancePre::with_clock(instance_pre, requires_async, clock.clone()),
        desc_vec.to_owned(),
    )
}

/// Compiles and pre-instantiates every module declared in the package
/// descriptor, returning them keyed by module name.
pub fn instantiate_component_modules(
    engine: &Engine,
    linker: &Linker<WasiContextComponent>,
    package: &AppPackage,
    clock: &Arc<EpochClock>,
) -> RS<Vec<(String, PackageModule)>> {
    let mut modules = Vec::new();

    let package_desc: &ModProcDesc = &package.package_desc;
    for (mod_name, vec_desc) in package_desc.modules() {
        let byte_code = package.modules.get(mod_name).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such module named {}", mod_name)
            )
        })?;
        let module =
            instantiate_component(clock, engine, linker, mod_name.clone(), byte_code, vec_desc)?;
        modules.push((mod_name.clone(), module));
    }
    Ok(modules)
}
