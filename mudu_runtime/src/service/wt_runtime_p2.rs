use crate::service::mudu_package::MuduPackage;
use crate::service::package_module::PackageModule;
use crate::service::wasi_context_p2::WasiContextP2;

use crate::service::runtime_opt::RuntimeOpt;
use crate::service::wasi_context_p2;
use crate::service::wt_instance_pre::WTInstancePre;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::procedure::package_desc::PackageDesc;
use mudu_contract::procedure::proc_desc::ProcDesc;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine};
use wasmtime_wasi::p2::add_to_linker_sync;

pub struct WTRuntimeP2 {
    #[allow(unused)]
    runtime_opt: RuntimeOpt,
    engine: Engine,
    linker:Linker<WasiContextP2>,
}

impl WTRuntimeP2 {
    pub fn build(runtime_opt: &RuntimeOpt) -> RS<Self> {
        let runtime_opt = runtime_opt.clone();
        let mut cfg = Config::new();
        cfg.wasm_component_model(true);
        if runtime_opt.enable_async {
            cfg.async_support(true)
                .wasm_component_model_async(true)
                .wasm_component_model_async_builtins(true);
        }
        let engine = Engine::new(&mut cfg)
            .map_err(|e| {
                m_error!(EC::InternalErr, "failed create new wasm runtime engine", e)
            })?;
        // Configure linker with host functions
        let linker = Linker::new(&engine);
        Ok(Self { runtime_opt, engine, linker })
    }

    pub fn instantiate(& mut self) -> RS<()> {
        wasi_context_p2::async_host::mududb::async_api::system::add_to_linker::<_, HasSelf<_>>(&mut self.linker, |c| { c})
            .map_err(|e| m_error!(EC::InternalErr, "instantiate, link async function error", e))?;
        wasi_context_p2::sync_host::mududb::api::system::add_to_linker::<_, HasSelf<_>>(&mut self.linker, |c| { c })
            .map_err(|e| m_error!(EC::InternalErr, "instantiate, link sync function error", e))?;
        add_to_linker_sync(&mut self.linker)
            .map_err(|e| m_error!(EC::MuduError, "wasmtime_wasi add_to_linker_sync error", e))?;
        Ok(())
    }

    pub fn compile_modules(&self, package: &MuduPackage) -> RS<Vec<(String, PackageModule)>> {
        let modules = instantiate_mpk_module_p2(&self.engine, &self.linker, package)?;
        Ok(modules)
    }
}


fn instantiate_component(
    engine: &Engine,
    linker: &Linker<WasiContextP2>,
    name: String,
    byte_code: &Vec<u8>,
    desc_vec: &Vec<ProcDesc>,
) -> RS<PackageModule> {
    let component = Component::from_binary(&engine, &byte_code).map_err(|e| {
        m_error!(
            EC::MuduError,
            format!("build component {} from binary error", name),
            e)
    })?;

    let instance_pre = linker.instantiate_pre(&component).map_err(|e| {
        m_error!(EC::MuduError,format!("instantiate module {} error", name), e)
    })?;

    PackageModule::new(WTInstancePre::from_p2(instance_pre), desc_vec.clone())
}


pub fn instantiate_mpk_module_p2(
    engine: &Engine,
    linker: &Linker<WasiContextP2>,
    package: &MuduPackage,
) -> RS<Vec<(String, PackageModule)>> {
    let mut modules = Vec::new();

    let package_desc: &PackageDesc = &package.package_desc;
    for (mod_name, vec_desc) in package_desc.modules() {
        let byte_code = package.modules.get(mod_name).ok_or_else(|| {
            m_error!(EC::NoneErr, format!("no such module named {}", mod_name))
        })?;
        let module =
            instantiate_component(engine, linker, mod_name.clone(), byte_code, vec_desc)?;
        modules.push((mod_name.clone(), module));
    }
    Ok(modules)
}


