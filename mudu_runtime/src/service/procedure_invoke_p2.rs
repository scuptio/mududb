use crate::procedure::procedure::Procedure;
use crate::service::wasi_context_p2::{build_wasi_p2_context, WasiContextP2};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::case_convert::to_kebab_case;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use std::sync::Mutex;
use std::thread;
use wasmtime::component::{InstancePre, TypedFunc};
use wasmtime::Store;

pub struct ProcedureInvokeP2 {
    inner: Mutex<ProcedureInvokeInner>,
}

impl ProcedureInvokeP2 {
    pub fn call(
        procedure: &Procedure,
        proc_opt: ProcOpt,
        param: ProcedureParam,
    ) -> RS<ProcedureResult> {
        let name = format!(
            "{}{}",
            mudu_contract::procedure::proc::MUDU_PROC_P2_PREFIX,
            procedure.proc_name()
        );
        let name = to_kebab_case(&name);
        let context = build_wasi_p2_context();
        let p = procedure.instance().as_p2_instance_pre();

        let this: Self = Self::new(context, p,  name, proc_opt)?;
        this.invoke(param)
    }

    pub async fn call_async(
        procedure: &Procedure,
        proc_opt: ProcOpt,
        param: ProcedureParam,
    ) -> RS<ProcedureResult> {
        let name = format!(
            "{}{}",
            mudu_contract::procedure::proc::MUDU_PROC_P2_PREFIX,
            procedure.proc_name()
        );
        let name = to_kebab_case(&name);
        let context = build_wasi_p2_context();
        let p = procedure.instance().as_p2_instance_pre();
        let this: Self = Self::new_async(context, p, name, proc_opt).await?;
        this.invoke_async(param).await
    }

    fn new(
        context: WasiContextP2,
        instance_pre: &InstancePre<WasiContextP2>,
        name:String,
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
        context: WasiContextP2,
        instance_pre: &InstancePre<WasiContextP2>,
        name:String,
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
            .map_err(|e| m_error!(EC::MuduError, "mutex into inner error", e))?;
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
            .map_err(|e| m_error!(EC::MuduError, "mutex into inner", e))?;
        inner.invoke_async(param).await
    }
}


#[allow(unused)]
struct ProcedureInvokeInner {
    store: Store<WasiContextP2>,
    typed_func: TypedFunc<(Vec<u8>, ), (Vec<u8>, )>,
    _proc_opt: ProcOpt,
}

const PAGE_SIZE: u64 = 65536;

#[allow(unused)]
pub struct ProcOpt {
    pub memory: u64,
    pub async_call: bool,
}

impl ProcOpt {
    #[allow(unused)]
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

impl ProcedureInvokeInner {
    fn new(
        context: WasiContextP2,
        instance_pre: &InstancePre<WasiContextP2>,
        name:String,
        proc_opt: ProcOpt,
    ) -> RS<ProcedureInvokeInner> {
        let mut store = Store::new(instance_pre.engine(), context);
        let instance = instance_pre.instantiate(&mut store)
            .map_err(|e|{m_error!(EC::InternalErr, "component instantiate error", e)})?;
        let function = instance.get_func(&mut store, &name)
            .map_or_else(|| {
                Err(m_error!(EC::InternalErr, format!("cannot get function named {}", name)))
            },
                         |f| { Ok(f) },
            )?;
        let typed_function = function.typed::<(Vec<u8>,), (Vec<u8>,)>(&mut store)
            .map_err(|e|{m_error!(EC::InternalErr, "get typed error", e)})?;
        Ok(Self {
            store,
            typed_func: typed_function,
            _proc_opt: proc_opt,
        })
    }

    async fn new_async(
        context: WasiContextP2,
        instance_pre: &InstancePre<WasiContextP2>,
        name:String,
        proc_opt: ProcOpt,
    ) -> RS<ProcedureInvokeInner> {
        let mut store = Store::new(instance_pre.engine(), context);
        let instance = instance_pre.instantiate_async(&mut store).await
            .map_err(|e|{m_error!(EC::InternalErr, "component instantiate error", e)})?;
        let function = instance.get_func(&mut store, &name)
            .map_or_else(|| {
                Err(m_error!(EC::InternalErr, "no function named {}", name))
            }, |f| {
                Ok(f)
            })?;
        let typed_function = function.typed::<(Vec<u8>,), (Vec<u8>,)>(&mut store)
            .map_err(|e| { m_error!(EC::InternalErr, "get typed async function error", e) })?;

        Ok(Self {
            store,
            typed_func:typed_function,
            _proc_opt: proc_opt,
        })
    }


    pub fn invoke(self, param: ProcedureParam) -> RS<ProcedureResult> {
        let param_p2 = procedure_invoke::serialize_param(param)?;
        let mut store = self.store;
        let (result_binary, ) = self.typed_func.call(&mut store, (param_p2,))
            .map_err(|e| { m_error!(EC::MuduError, "invoke call error", e) })?;
        let result_p2 = procedure_invoke::deserialize_result(&result_binary)?;
        Ok(result_p2)
    }

    pub async fn invoke_async(self, param: ProcedureParam) -> RS<ProcedureResult> {
        let param_p2 = procedure_invoke::serialize_param(param)?;
        let mut store = self.store;
        let (result_binary, ) = self.typed_func.call_async(&mut store, (param_p2,))
            .await
            .map_err(|e| { m_error!(EC::MuduError, "invoke call async error", e) })?;
        let result_p2 = procedure_invoke::deserialize_result(&result_binary)?;
        Ok(result_p2)
    }
}
