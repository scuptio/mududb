use crate::procedure::procedure::Procedure;
use crate::procedure::wasi_context::{build_wasi_p1_context, WasiContext};
use anyhow::Context;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use std::sync::Mutex;
use std::thread;
use wasmtime::{InstancePre, Memory, Store, TypedFunc};

pub struct ProcedureInvoke1 {
    inner: Mutex<ProcedureInvokeInner>,
}

impl ProcedureInvoke1 {
    pub fn call(
        procedure: &Procedure,
        proc_opt: ProcOpt,
        param: ProcedureParam,
    ) -> RS<ProcedureResult> {
        let name = format!(
            "{}{}",
            mudu_contract::procedure::proc::MUDU_PROC_PREFIX,
            procedure.proc_name()
        );
        let context = build_wasi_p1_context();
        let this: Self = Self::new(context, procedure.instance().as_p1_instance_pre(), name, proc_opt)?;
        this.invoke(param)
    }

    #[allow(unused)]
    pub async fn call_async(
        procedure: &Procedure,
        proc_opt: ProcOpt,
        param: ProcedureParam,
    ) -> RS<ProcedureResult> {
        let name = format!(
            "{}{}",
            mudu_contract::procedure::proc::MUDU_PROC_PREFIX,
            procedure.proc_name()
        );
        let context = build_wasi_p1_context();
        let this: Self = Self::new_async(context, procedure.instance().as_p1_instance_pre(), name, proc_opt).await?;
        this.invoke_async(param).await
    }

    fn new(
        context: WasiContext,
        instance_pre: &InstancePre<WasiContext>,
        name: String,
        proc_opt: ProcOpt,
    ) -> RS<Self> {
        Ok(Self {
            inner: Mutex::new(ProcedureInvokeInner::new(
                context,
                instance_pre,
                name,
                proc_opt,
            )?),
        })
    }

    async fn new_async(
        context: WasiContext,
        instance_pre: &InstancePre<WasiContext>,
        name: String,
        proc_opt: ProcOpt,
    ) -> RS<Self> {
        Ok(Self {
            inner: Mutex::new(ProcedureInvokeInner::new_async(
                context,
                instance_pre,
                name,
                proc_opt,
            ).await?),
        })
    }
    fn invoke(self, param: ProcedureParam) -> RS<ProcedureResult> {
        let inner = self.inner;
        let inner: ProcedureInvokeInner = inner
            .into_inner()
            .map_err(|e| m_error!(EC::MuduError, "", e))?;
        let thread = thread::spawn(move || {
            let ret = inner.invoke(param);
            ret
        });
        let result = thread
            .join()
            .map_err(|_e| m_error!(EC::MuduError, "invoke thread join error"))?;
        result
    }

    async fn invoke_async(self, param: ProcedureParam) -> RS<ProcedureResult> {
        let inner = self.inner;
        let inner: ProcedureInvokeInner = inner
            .into_inner()
            .map_err(|e| m_error!(EC::MuduError, "", e))?;
        inner.invoke_async(param).await
    }
}

struct ProcedureInvokeInner {
    store: Store<WasiContext>,
    typed_func: TypedFunc<(u32, u32, u32, u32), i32>,
    _proc_opt: ProcOpt,
    memory: Memory,
}

const PAGE_SIZE: u64 = 65536;

#[allow(unused)]
pub struct ProcOpt {
    pub memory: u64,
    pub async_call: bool,
}

impl ProcOpt {
    fn memory_size(&self) -> u64 {
        self.memory
    }
}

impl Default for ProcOpt {
    fn default() -> Self {
        Self {
            memory: PAGE_SIZE * 2000,
            async_call: false,
        }
    }
}

struct InvokeParam {
    in_ptr: u32,
    in_size: u32,
    out_ptr: u32,
    out_size: u32,
}

fn page_align_size(size: u64) -> u64 {
    (size + PAGE_SIZE - 1) / PAGE_SIZE
}

impl ProcedureInvokeInner {
    fn new(
        context: WasiContext,
        instance_pre: &InstancePre<WasiContext>,
        name: String,
        proc_opt: ProcOpt,
    ) -> RS<ProcedureInvokeInner> {
        let mut store = Store::new(instance_pre.module().engine(), context);
        let instance = instance_pre
            .instantiate(&mut store)
            .expect(&format!("failed to instantiate procedure: {}", name));
        let typed_func = instance
            .get_typed_func::<(u32, u32, u32, u32), i32>(&mut store, &name)
            .expect(&format!("get_typed_func: {}", name));
        let memory = instance
            .get_memory(&mut store, "memory")
            .context("Memory not found".to_string())
            .map_err(|e| m_error!(EC::MuduError, "", e))?;

        let size = page_align_size(proc_opt.memory_size());
        memory
            .grow(&mut store, size)
            .map_err(|e| m_error!(EC::MuduError, "", e))?;

        Ok(Self {
            store,
            typed_func,
            _proc_opt: proc_opt,
            memory,
        })
    }

    async fn new_async(
        context: WasiContext,
        instance_pre: &InstancePre<WasiContext>,
        name: String,
        proc_opt: ProcOpt,
    ) -> RS<ProcedureInvokeInner> {
        let mut store = Store::new(instance_pre.module().engine(), context);
        let instance = instance_pre
            .instantiate_async(&mut store).await
            .expect(&format!("failed to instantiate procedure: {}", name));
        let typed_func = instance
            .get_typed_func::<(u32, u32, u32, u32), i32>(&mut store, &name)
            .expect(&format!("get_typed_func: {}", name));
        let memory = instance
            .get_memory(&mut store, "memory")
            .context("Memory not found".to_string())
            .map_err(|e| m_error!(EC::MuduError, "", e))?;

        let size = page_align_size(proc_opt.memory_size());
        memory
            .grow(&mut store, size)
            .map_err(|e| m_error!(EC::MuduError, "", e))?;

        Ok(Self {
            store,
            typed_func,
            _proc_opt: proc_opt,
            memory,
        })
    }
    fn process_invoke_param(&mut self, param: ProcedureParam) -> RS<InvokeParam> {
        let buf = self.memory.data_mut(&mut self.store);
        let param_b = procedure_invoke::serialize_param(param)?;
        if param_b.len() > buf.len() {
            return Err(m_error!(
                EC::InsufficientBufferSpace, format!(
                    "failed to serialize procedure: buffer size not efficient {}",
                    buf.len()
                )
            ));
        }
        buf.copy_from_slice(&param_b);
        let in_ptr = 0u32;
        let in_size = param_b.len() as u32;
        let out_ptr = in_size;
        let out_size = buf.len() as u32 - out_ptr;
        Ok(InvokeParam {
            in_ptr,
            in_size,
            out_ptr,
            out_size,
        })
    }


    fn process_invoke_result(&mut self, invoke_param: &InvokeParam, r: anyhow::Result<i32>) -> RS<ProcedureResult> {
        match r {
            Ok(code) => {
                if code == 0 {
                    let buf = self.memory.data_mut(&mut self.store);
                    let buf = &buf[invoke_param.out_ptr as usize..invoke_param.out_size as usize];
                    let result = procedure_invoke::deserialize_result(buf)?;
                    Ok(result)
                } else {
                    Err(m_error!(
                        EC::MuduError,
                        format!("procedure invoke error, returned code {}", code)
                    ))
                }
            }
            Err(e) => Err(m_error!(EC::MuduError, "", e)),
        }
    }

    pub fn invoke(self, param: ProcedureParam) -> RS<ProcedureResult> {
        let mut this = self;
        let invoke_param = this.process_invoke_param(param)?;
        let r = this
            .typed_func
            .call(&mut this.store, (
                invoke_param.in_ptr,
                invoke_param.in_size,
                invoke_param.out_ptr,
                invoke_param.out_size
            ));
        this.process_invoke_result(&invoke_param, r)
    }

    pub async fn invoke_async(self, param: ProcedureParam) -> RS<ProcedureResult> {
        let mut this = self;
        let invoke_param = this.process_invoke_param(param)?;
        let r = this
            .typed_func
            .call_async(&mut this.store, (
                invoke_param.in_ptr,
                invoke_param.in_size,
                invoke_param.out_ptr,
                invoke_param.out_size
            )).await;
        this.process_invoke_result(&invoke_param, r)
    }
}
