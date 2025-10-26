use crate::procedure::wasi_context::WasiContext;
use crate::service::app_inst::AppInst;
use crate::service::app_inst_impl::AppInstImpl;
use crate::service::app_package::AppPackage;
use crate::service::{file_name, kernel_function};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use scc::HashMap as SCCHashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use wasmtime::{Caller, Engine, Linker};

pub struct RuntimeSimple {
    db_path: String,
    package_path: String,
    engine: Engine,
    apps: SCCHashMap<String, AppInstImpl>,
    linker: Linker<WasiContext>,
}

fn load_package_files<P1: AsRef<Path>, F: Fn(&str) -> RS<()>>(
    package_dir_path: P1,
    handle_package_file: &F,
) -> RS<()> {
    let dir = package_dir_path.as_ref();
    for entry in fs::read_dir(&dir)
        .map_err(|e| {
            m_error!(EC::MuduError, format!("read directory {:?} error", dir), e)
        })?
    {
        let entry = entry
            .map_err(|e| {
                m_error!(EC::MuduError, "entry  error", e)
            })?;
        let path = entry.path();

        // check file name
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext.to_ascii_lowercase() == file_name::APP_PACKAGE_EXTENSION {
                    let path_str = path.as_path().to_str()
                        .ok_or_else(|| { m_error!(EC::IOErr, format!("path {:?} to str error", path)) })?;
                    handle_package_file(path_str)?;
                }
            }
        }
    }

    Ok(())
}

impl RuntimeSimple {
    pub fn new(
        package_path: &String,
        db_path: &String,
    ) -> RuntimeSimple {
        let engine = Engine::default();
        // Configure linker with host functions
        let linker = Linker::new(&engine);
        Self {
            package_path: package_path.clone(),
            db_path: db_path.clone(),
            engine,
            apps: Default::default(),
            linker,
        }
    }

    pub fn initialized(&mut self) -> RS<()> {
        Self::register_sys_call(&mut self.linker)?;
        wasmtime_wasi::p1::add_to_linker_sync(&mut self.linker, |ctx|
            {
                ctx.wasi_mut()
            })
            .map_err(|e| {
                m_error!(EC::MuduError, "wasmtime_wasi add_to_linker_sync error", e)
            })?;
        if !fs::exists(&self.db_path).map_err(|e| {
            m_error!(EC::IOErr, "test db directory exists error", e)
        })? {
            fs::create_dir_all(&self.db_path).map_err(|e| {
                m_error!(EC::IOErr, format!("create directory {} error", self.db_path), e)
            })?
        } else if let metadata = fs::metadata(&self.db_path).map_err(|e| {
            m_error!(EC::IOErr, "read db metadata error", e)
        })? && metadata.is_file() {
            return Err(m_error!(EC::IOErr, format!("path {} is a not a directory", self.db_path)));
        }

        load_package_files(
            &self.package_path,
            &|path| {
                let app_package = AppPackage::load(path)?;
                let app_instance = AppInstImpl::build(
                    &self.engine, &self.linker, &self.db_path, app_package)?;
                let _ = self.apps.insert_sync(app_instance.name().to_string(), app_instance);
                Ok(())
            },
        )?;
        Ok(())
    }

    fn register_sys_call(linker: &mut Linker<WasiContext>) -> RS<()> {
        let module_name = "env";
        linker.func_wrap(
            module_name, "query",
            |caller: Caller<'_, WasiContext>,
             param_buf_ptr: u32,
             param_buf_len: u32,
             out_buf_ptr: u32,
             out_buf_len: u32,
             out_mem_ptr: u32,
             out_mem_len: u32,
            | -> i32 {
                kernel_function::kernel_query(
                    caller,
                    param_buf_ptr,
                    param_buf_len,
                    out_buf_ptr,
                    out_buf_len,
                    out_mem_ptr,
                    out_mem_len,
                )
            },
        ).map_err(|e| { m_error!(EC::MuduError, "register query error", e) })?;

        linker.func_wrap(
            module_name, "sys_command",
            |caller: Caller<'_, WasiContext>,
             param_buf_ptr: u32,
             param_buf_len: u32,
             out_buf_ptr: u32,
             out_buf_len: u32,
             out_mem_ptr: u32,
             out_mem_len: u32,
            | -> i32 {
                kernel_function::kernel_command(
                    caller,
                    param_buf_ptr,
                    param_buf_len,
                    out_buf_ptr,
                    out_buf_len,
                    out_mem_ptr,
                    out_mem_len,
                )
            },
        ).map_err(|e| { m_error!(EC::MuduError, "register command error", e) })?;

        linker.func_wrap(
            module_name, "sys_fetch",
            |caller: Caller<'_, WasiContext>,
             param_buf_ptr: u32,
             param_buf_len: u32,
             out_buf_ptr: u32,
             out_buf_len: u32,
             out_mem_ptr: u32,
             out_mem_len: u32,
            | -> i32 {
                kernel_function::kernel_fetch(
                    caller,
                    param_buf_ptr,
                    param_buf_len,
                    out_buf_ptr,
                    out_buf_len,
                    out_mem_ptr,
                    out_mem_len,
                )
            },
        ).map_err(|e| { m_error!(EC::MuduError, "register fetch error", e) })?;

        linker.func_wrap(
            module_name, "sys_get_memory",
            |caller: Caller<'_, WasiContext>,
             mem_id: u32,
             out_buf_ptr: u32,
             out_buf_len: u32| -> i32 {
                kernel_function::kernel_get_memory(
                    caller,
                    mem_id,
                    out_buf_ptr,
                    out_buf_len,
                )
            },
        ).map_err(|e| { m_error!(EC::MuduError, "", e) })?;

        Ok(())
    }

    pub fn app(&self, name: &String) -> Option<Arc<dyn AppInst>> {
        self.apps.get_sync(name).map(|e| {
            Arc::new(e.get().clone()) as Arc<dyn AppInst>
        })
    }
}


#[cfg(test)]
mod tests {
    use crate::service::service::Service;
    use crate::service::service_impl::create_runtime_service;
    use crate::service::test_wasm_mod_path::wasm_mod_path;
    use mudu::common::result::RS;
    use mudu::error::ec::EC;
    use mudu::m_error;
    use mudu::procedure::proc_param::ProcParam;
    use mudu::tuple::rs_tuple_datum::RsTupleDatum;
    use mudu_utils::notifier::Notifier;
    use mudu_utils::task::{spawn_task, this_task_id};
    use std::sync::Arc;

    ///
    /// See proc function definition [proc](mudu_wasm/src/wasm/proc.rs#L5)。
    ///
    #[test]
    fn test_runtime_simple() {
        tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(async {
            let r = test_async_runtime_simple().await;
            println!("{:?}", r);
        });
    }

    async fn test_async_runtime_simple() -> RS<()> {
        let path = wasm_mod_path();
        let service = create_runtime_service(&path, &path).unwrap();

        let stopper = Notifier::new();
        let task = spawn_task(stopper.clone(), "test session task", async move {
            async_session(service).await?;
            Ok(())
        })?;
        let opt = task.await.map_err(|e|
            m_error!(EC::InternalErr, "join error", e)
        )?;
        match opt {
            Some(r) => { r }
            None => { Ok(()) }
        }
    }

    async fn async_session(service: Arc<dyn Service>) -> RS<()> {
        println!("task id {}", this_task_id());
        let tuple = (1i32, 100i64, "string argument".to_string());
        let desc = <(i32, i64, String)>::tuple_desc_static();
        let params = ProcParam::from_tuple(0, tuple, &desc)?;
        let app_name = "app1".to_string();
        let app = service.app(&app_name).ok_or(m_error!(EC::NoneErr, format!("no such app named {}", app_name)))?;
        app.task_create()?;
        let proc_result = app.invoke(&"mod_0".to_string(), &"proc".to_string(), params)?;
        let result = proc_result.to::<(i32, String)>(&<(i32, String)>::tuple_desc_static())?;
        println!("result: {:?}", result);
        app.task_end()?;
        Ok(())
    }
}
