//! Pool of instantiated WASM component instances for procedure invocation.
//!
//! Creating a fresh [`Store`] and instantiating the component on every
//! procedure call costs far more than the call itself (linear memory
//! allocation, WASI context build, linking). This pool keeps idle instances
//! per exported function and reuses them across invocations.
//!
//! Semantic note: a reused instance keeps its guest linear memory and globals
//! from previous invocations. Procedures must therefore be written as
//! stateless functions of their parameters (the mudu binding contract), and
//! must not rely on fresh-instance state. An instance whose call traps is
//! discarded instead of being returned to the pool, so a poisoned store is
//! never reused. A clean return carrying a domain error (an abort, encoded in
//! the result bytes) leaves the store healthy and the instance is reused.

#![allow(missing_docs)]

use crate::service::wasi_context_component::{WasiContextComponent, build_wasi_component_context};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use mudu_kernel::server::worker_local::WorkerLocalRef;
use mudu_sys::sync::SMutex;
use std::collections::HashMap;
use std::sync::Arc;
use wasmtime::Store;
use wasmtime::component::{InstancePre, TypedFunc};

type ProcFunc = TypedFunc<(Vec<u8>,), (Vec<u8>,)>;

/// Maximum idle instances kept per exported function. Instances returned
/// beyond this cap are dropped to bound pooled guest memory.
const MAX_IDLE_PER_FUNCTION: usize = 64;

struct PooledInstance {
    store: Store<WasiContextComponent>,
    typed_func: ProcFunc,
}

pub struct ProcedureInstancePool {
    idle: SMutex<HashMap<String, Vec<PooledInstance>>>,
}

impl Default for ProcedureInstancePool {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcedureInstancePool {
    pub fn new() -> Self {
        Self {
            idle: SMutex::new(HashMap::new()),
        }
    }

    /// Lease an instance for `func_name`, instantiating a new one when the
    /// pool has none idle.
    pub async fn lease(
        self: &Arc<Self>,
        instance_pre: &InstancePre<WasiContextComponent>,
        func_name: &str,
    ) -> RS<LeasedInstance> {
        let pooled = {
            let mut idle = self
                .idle
                .lock()
                .map_err(|e| mudu_error!(ErrorCode::Mutex, "instance pool lock", e))?;
            idle.get_mut(func_name).and_then(Vec::pop)
        };
        let pooled = match pooled {
            Some(pooled) => pooled,
            None => Self::instantiate(instance_pre, func_name).await?,
        };
        Ok(LeasedInstance {
            pool: self.clone(),
            func_name: func_name.to_string(),
            inner: Some(pooled),
        })
    }

    async fn instantiate(
        instance_pre: &InstancePre<WasiContextComponent>,
        func_name: &str,
    ) -> RS<PooledInstance> {
        let mut store = Store::new(instance_pre.engine(), build_wasi_component_context(None));
        let instance = instance_pre
            .instantiate_async(&mut store)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Internal, "component instantiate error", e))?;
        let function = instance.get_func(&mut store, func_name).ok_or_else(|| {
            mudu_error!(
                ErrorCode::Internal,
                format!("no function named {}", func_name)
            )
        })?;
        let typed_func = function
            .typed::<(Vec<u8>,), (Vec<u8>,)>(&mut store)
            .map_err(|e| mudu_error!(ErrorCode::Internal, "get typed async function error", e))?;
        Ok(PooledInstance { store, typed_func })
    }

    fn return_instance(&self, func_name: &str, pooled: PooledInstance) {
        let Ok(mut idle) = self.idle.lock() else {
            return;
        };
        let list = idle.entry(func_name.to_string()).or_default();
        if list.len() < MAX_IDLE_PER_FUNCTION {
            list.push(pooled);
        }
    }

    #[cfg(test)]
    pub fn idle_len(&self, func_name: &str) -> usize {
        self.idle
            .lock()
            .map(|idle| idle.get(func_name).map_or(0, Vec::len))
            .unwrap_or(0)
    }
}

impl Drop for LeasedInstance {
    fn drop(&mut self) {
        // A lease that was never invoked (e.g. parameter serialization
        // failed) still holds a clean instance: hand it back to the pool.
        if let Some(pooled) = self.inner.take() {
            self.pool.return_instance(&self.func_name, pooled);
        }
    }
}

/// An instance checked out from the pool. On a clean invocation the instance
/// is handed back to the pool; after a trap it is dropped.
pub struct LeasedInstance {
    pool: Arc<ProcedureInstancePool>,
    func_name: String,
    inner: Option<PooledInstance>,
}

impl LeasedInstance {
    /// Point the WASI host context at the caller's worker. Must be called
    /// before every invoke: leased instances are shared across workers.
    pub fn set_worker_local(&mut self, worker_local: Option<WorkerLocalRef>) {
        if let Some(pooled) = self.inner.as_mut() {
            pooled.store.data_mut().set_worker_local(worker_local);
        }
    }

    pub async fn invoke(mut self, param: ProcedureParam) -> RS<ProcedureResult> {
        let param_p2 = procedure_invoke::serialize_param(param)?;
        let mut pooled = self
            .inner
            .take()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "leased instance already consumed"))?;
        let call_result = pooled
            .typed_func
            .call_async(&mut pooled.store, (param_p2,))
            .await;
        match call_result {
            Ok((result_binary,)) => {
                // The guest returned cleanly. Its result bytes encode either
                // a value or a domain error (see `serialize_result` taking an
                // `RS<ProcedureResult>`), so a deserialize `Err` here is the
                // procedure's own error — not a poisoned store. The instance
                // stays reusable either way.
                let result = procedure_invoke::deserialize_result(&result_binary);
                self.pool.return_instance(&self.func_name, pooled);
                result
            }
            Err(e) => {
                // The guest trapped: the store may be poisoned, discard it.
                Err(mudu_error!(
                    ErrorCode::DomainViolation,
                    "invoke call async error",
                    e
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::procedure::procedure::Procedure;
    use crate::service::app_package::AppPackage;
    use crate::service::procedure_invoke_component::{ProcOpt, ProcedureInvokeComponent};
    use crate::service::runtime_opt::{ComponentTarget, RuntimeOpt};
    use crate::service::test_wasm_mod_path::wasm_mod_path;
    use crate::service::wt_runtime_component::WTRuntimeComponent;
    use mudu::utils::case_convert::to_kebab_case;
    use mudu_contract::procedure::procedure_param::ProcedureParam;
    use mudu_type::data_value::DataValue;
    use std::path::PathBuf;

    fn get_procedure(proc_name: &str) -> Procedure {
        let package = AppPackage::load(PathBuf::from(wasm_mod_path()).join("app1.mpk")).unwrap();
        let mut runtime = WTRuntimeComponent::build(&RuntimeOpt {
            component_target: ComponentTarget::P2,
            enable_async: true,
            sever_mode: Default::default(),
            async_runtime: None,
        })
        .unwrap();
        runtime.instantiate().unwrap();
        let modules = runtime.compile_modules(&package).unwrap();
        modules
            .into_iter()
            .find(|(name, _)| name == "mod_0")
            .unwrap()
            .1
            .procedure(proc_name)
            .unwrap()
    }

    fn sample_param() -> ProcedureParam {
        ProcedureParam::new(
            0,
            0,
            vec![
                DataValue::from_i32(2),
                DataValue::from_i64(3),
                DataValue::from_string("hello".to_string()),
            ],
        )
    }

    /// Two invocations of the same procedure must reuse one pooled instance:
    /// after both calls exactly one idle instance sits in the pool (a broken
    /// pool would show two, one per instantiation).
    #[tokio::test]
    #[cfg_attr(miri, ignore)]
    async fn pooled_instance_is_reused_across_calls() {
        let proc = get_procedure("proc2_mtp");
        let func_name = to_kebab_case(&format!(
            "{}{}",
            mudu_contract::procedure::proc::MUDU_PROC_P2_PREFIX,
            proc.proc_name()
        ));
        for _ in 0..2 {
            ProcedureInvokeComponent::call_async(
                &proc,
                ComponentTarget::P2,
                ProcOpt::default(),
                sample_param(),
                None,
            )
            .await
            .expect("procedure call should succeed");
        }
        assert_eq!(proc.instance().pool_idle_len(&func_name), 1);
    }
}
