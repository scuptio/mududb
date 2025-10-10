use crate::procedure::wasi_context::WasiContext;
use anyhow::Context;
use mudu::common::result::RS;
use mudu::common::serde_utils::{
    deserialize_sized_from,
    serialize_sized_to,
    size_len,
};
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::procedure::proc_param::ProcParam;
use mudu::procedure::proc_result::ProcResult;
use std::sync::Mutex;
use wasmtime::{InstancePre, Memory, Store, TypedFunc};

pub struct ProcedureInvoke {
    inner: Mutex<ProcedureInvokeInner>,
}

impl ProcedureInvoke {
    pub fn call(
        context: WasiContext,
        instance_pre: &InstancePre<WasiContext>,
        proc_opt: ProcOpt,
        name: String,
        param: ProcParam,
    ) -> RS<ProcResult> {
        let this: Self = Self::new(
            context,
            instance_pre,
            name,
            proc_opt,
        )?;
        this.invoke(param)
    }

    fn new(
        context: WasiContext,
        instance_pre: &InstancePre<WasiContext>,
        name: String,
        proc_opt: ProcOpt,
    ) -> RS<Self> {
        Ok(Self {
            inner: Mutex::new(
                ProcedureInvokeInner::new(
                    context,
                    instance_pre,
                    name,
                    proc_opt,
                )?)
        })
    }

    fn invoke(self, param: ProcParam) -> RS<ProcResult> {
        let inner = self.inner;
        let inner: ProcedureInvokeInner = inner
            .into_inner()
            .map_err(|e| {
                m_error!(EC::MuduError, "", e)
            })?;
        inner.invoke(param)
    }
}

struct ProcedureInvokeInner {
    store: Store<WasiContext>,
    typed_func: TypedFunc<(u32, u32, u32, u32), i32>,
    proc_opt: ProcOpt,
    memory: Memory,
}

const PAGE_SIZE: u64 = 65536;

pub struct ProcOpt {
    pub memory: u64,
}

impl ProcOpt {
    fn memory_size(&self) -> u64 {
        self.memory
    }
}

impl Default for ProcOpt {
    fn default() -> Self {
        Self {
            memory: PAGE_SIZE * 2,
        }
    }
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
        let instance = instance_pre.instantiate(&mut store)
            .expect(&format!("failed to instantiate procedure: {}", name));
        let typed_func = instance.get_typed_func::<(u32, u32, u32, u32), i32>(&mut store, &name)
            .expect(&format!("get_typed_func: {}", name));
        let memory = instance.get_memory(&mut store, "memory")
            .context("Memory not found".to_string())
            .map_err(|e| { m_error!(EC::MuduError, "", e) })?;

        let size = page_align_size(proc_opt.memory_size());
        memory.grow(&mut store, size)
            .map_err(|e| { m_error!(EC::MuduError, "", e) })?;

        Ok(Self {
            store,
            typed_func,
            proc_opt,
            memory,
        })
    }

    pub fn invoke(self, param: ProcParam) -> RS<ProcResult> {
        let mut this = self;
        let (in_ptr, in_size, out_ptr, out_size) = {
            let buf = this.memory.data_mut(&mut this.store);
            let (ok, size) = serialize_sized_to(&param, buf)
                .map_err(|e| {
                    m_error!(EC::MuduError, "", e)
                })?;
            if !ok {
                return Err(m_error!(EC::MuduError, format!("failed to serialize procedure: buffer size not efficient {}", buf.len())));
            }
            let in_ptr = 0;
            let in_size = size + size_len();
            let out_ptr = in_size;
            let out_size = buf.len() as u64 - out_ptr;
            (in_ptr as u32, in_size as u32, out_ptr as u32, out_size as u32)
        };
        let r = this.typed_func.call(&mut this.store, (in_ptr, in_size, out_ptr, out_size));
        match r {
            Ok(code) => {
                if code == 0 {
                    let buf = this.memory.data_mut(&mut this.store);
                    let buf = &buf[out_ptr as usize..out_size as usize];
                    let (result, _) = deserialize_sized_from::<ProcResult>(buf)?;
                    Ok(result)
                } else {
                    Err(m_error!(EC::MuduError, format!("procedure invoke error, returned code {}", code)))
                }
            }
            Err(e) => {
                Err(m_error!(EC::MuduError, "", e))
            }
        }
    }
}
