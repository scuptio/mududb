use crate::procedure::wasi_context::WasiContext;
use crate::service::kernel_function_p1;
use crate::service::mudu_package::MuduPackage;
use crate::service::package_module::PackageModule;
use crate::service::wt_instance_pre::WTInstancePre;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::procedure::package_desc::PackageDesc;
use mudu_contract::procedure::proc_desc::ProcDesc;
use wasmtime::{Caller, Config, Engine, Linker, Module};


pub struct WTRuntimeP1 {
    engine: Engine,
    linker:Linker<WasiContext>,
}

impl WTRuntimeP1 {
    pub fn build() -> RS<Self> {
        let mut cfg = Config::new();
        cfg.async_support(false);
        let engine = Engine::new(&mut cfg)
            .map_err(|e| {
                m_error!(EC::InternalErr, "failed create new wasm runtime engine", e)
            })?;
        // Configure linker with host functions
        let linker = Linker::new(&engine);
        Ok(Self {engine, linker})
    }

    pub fn instantiate(& mut self) -> RS<()> {
        register_sys_call(&mut self.linker)?;
        wasmtime_wasi::p1::add_to_linker_sync(&mut self.linker, |ctx| ctx.wasi_mut())
            .map_err(|e| m_error!(EC::MuduError, "wasmtime_wasi add_to_linker_sync error", e))?;
        Ok(())
    }

    pub fn compile_modules(&self, package: &MuduPackage) -> RS<Vec<(String, PackageModule)>> {
        let modules = instantiate_mpk_modules(&self.engine, &self.linker, package)?;
        Ok(modules)
    }
}

fn instantiate_module(
    engine: &Engine,
    linker: &Linker<WasiContext>,
    name: String,
    byte_code: &Vec<u8>,
    desc_vec: &Vec<ProcDesc>,
) -> RS<PackageModule> {
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
    PackageModule::new(WTInstancePre::from_p1(instance_pre), desc_vec.clone())
}


fn instantiate_mpk_modules(
    engine: &Engine,
    linker: &Linker<WasiContext>,
    package: &MuduPackage,
) -> RS<Vec<(String, PackageModule)>> {
    let mut modules = Vec::new();
    let app_proc_desc: &PackageDesc = &package.package_desc;
    for (mod_name, vec_desc) in app_proc_desc.modules() {
        let byte_code = package.modules.get(mod_name).ok_or_else(|| {
            m_error!(EC::NoneErr, format!("no such module named {}", mod_name))
        })?;
        let module =
            instantiate_module(engine, linker, mod_name.clone(), byte_code, vec_desc)?;
        modules.push((mod_name.clone(), module));
    }
    Ok(modules)
}



fn register_sys_call(linker: &mut Linker<WasiContext>) -> RS<()> {
    let module_name = "env";
    linker
        .func_wrap(
            module_name,
            "sys_query",
            |caller: Caller<'_, WasiContext>,
             param_buf_ptr: u32,
             param_buf_len: u32,
             out_buf_ptr: u32,
             out_buf_len: u32,
             out_mem_ptr: u32,
             out_mem_len: u32|
             -> i32 {
                kernel_function_p1::kernel_query(
                    caller,
                    param_buf_ptr,
                    param_buf_len,
                    out_buf_ptr,
                    out_buf_len,
                    out_mem_ptr,
                    out_mem_len,
                )
            },
        )
        .map_err(|e| m_error!(EC::MuduError, "register query error", e))?;

    linker
        .func_wrap(
            module_name,
            "sys_command",
            |caller: Caller<'_, WasiContext>,
             param_buf_ptr: u32,
             param_buf_len: u32,
             out_buf_ptr: u32,
             out_buf_len: u32,
             out_mem_ptr: u32,
             out_mem_len: u32|
             -> i32 {
                kernel_function_p1::kernel_command_p1(
                    caller,
                    param_buf_ptr,
                    param_buf_len,
                    out_buf_ptr,
                    out_buf_len,
                    out_mem_ptr,
                    out_mem_len,
                )
            },
        )
        .map_err(|e| m_error!(EC::MuduError, "register command error", e))?;

    linker
        .func_wrap(
            module_name,
            "sys_fetch",
            |caller: Caller<'_, WasiContext>,
             param_buf_ptr: u32,
             param_buf_len: u32,
             out_buf_ptr: u32,
             out_buf_len: u32,
             out_mem_ptr: u32,
             out_mem_len: u32|
             -> i32 {
                kernel_function_p1::kernel_fetch_p1(
                    caller,
                    param_buf_ptr,
                    param_buf_len,
                    out_buf_ptr,
                    out_buf_len,
                    out_mem_ptr,
                    out_mem_len,
                )
            },
        )
        .map_err(|e| m_error!(EC::MuduError, "register fetch error", e))?;

    linker
        .func_wrap(
            module_name,
            "sys_get_memory",
            |caller: Caller<'_, WasiContext>,
             mem_id: u32,
             out_buf_ptr: u32,
             out_buf_len: u32|
             -> i32 {
                kernel_function_p1::kernel_get_memory_p1(caller, mem_id, out_buf_ptr, out_buf_len)
            },
        )
        .map_err(|e| m_error!(EC::MuduError, "", e))?;

    Ok(())
}