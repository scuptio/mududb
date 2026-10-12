//! Fresh Store and component instance per invocation; only compiled code is reused.

use super::invocation_limits::InvocationContext;
use crate::service::wasi_context_component::{WasiContextComponent, build_wasi_component_context};
use ::conn::worker_local::WorkerLocalRef;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use std::sync::Arc;
use wasmtime::Store;
use wasmtime::component::{InstancePre, TypedFunc};

type ProcFunc = TypedFunc<(Vec<u8>,), (Vec<u8>,)>;
struct PooledInstance {
    store: Store<WasiContextComponent>,
    typed_func: ProcFunc,
}

/// Factory retained under its historical name; live instances are never pooled.
#[derive(Default)]
pub struct ProcedureInstancePool;
impl ProcedureInstancePool {
    /// Creates an instance factory.
    pub fn new() -> Self {
        Self
    }

    /// Creates a fresh asynchronous instance with default budgets.
    pub async fn lease(
        self: &Arc<Self>,
        pre: &InstancePre<WasiContextComponent>,
        name: &str,
    ) -> RS<LeasedInstance> {
        self.lease_with_context(pre, name, InvocationContext::new(Default::default())?, None)
            .await
    }

    pub(crate) async fn lease_with_context(
        self: &Arc<Self>,
        pre: &InstancePre<WasiContextComponent>,
        name: &str,
        control: Arc<InvocationContext>,
        worker: Option<WorkerLocalRef>,
    ) -> RS<LeasedInstance> {
        let mut store = Self::store(pre, control, worker, true)?;
        let instance = pre.instantiate_async(&mut store).await.map_err(|e| {
            mudu_error!(
                ErrorCode::DomainViolation,
                "component initialization failed",
                e
            )
        })?;
        let typed_func = instance
            .get_typed_func::<(Vec<u8>,), (Vec<u8>,)>(&mut store, name)
            .map_err(|e| {
                mudu_error!(
                    ErrorCode::Internal,
                    format!("procedure export missing: {name}"),
                    e
                )
            })?;
        Ok(LeasedInstance {
            inner: Some(PooledInstance { store, typed_func }),
        })
    }

    /// Creates a fresh synchronous instance with default budgets.
    pub fn lease_sync(
        self: &Arc<Self>,
        pre: &InstancePre<WasiContextComponent>,
        name: &str,
    ) -> RS<LeasedInstance> {
        self.lease_sync_with_context(pre, name, InvocationContext::new(Default::default())?, None)
    }

    pub(crate) fn lease_sync_with_context(
        self: &Arc<Self>,
        pre: &InstancePre<WasiContextComponent>,
        name: &str,
        control: Arc<InvocationContext>,
        worker: Option<WorkerLocalRef>,
    ) -> RS<LeasedInstance> {
        let mut store = Self::store(pre, control, worker, false)?;
        let instance = pre.instantiate(&mut store).map_err(|e| {
            mudu_error!(
                ErrorCode::DomainViolation,
                "component initialization failed",
                e
            )
        })?;
        let typed_func = instance
            .get_typed_func::<(Vec<u8>,), (Vec<u8>,)>(&mut store, name)
            .map_err(|e| {
                mudu_error!(
                    ErrorCode::Internal,
                    format!("procedure export missing: {name}"),
                    e
                )
            })?;
        Ok(LeasedInstance {
            inner: Some(PooledInstance { store, typed_func }),
        })
    }

    fn store(
        pre: &InstancePre<WasiContextComponent>,
        control: Arc<InvocationContext>,
        worker: Option<WorkerLocalRef>,
        asynchronous: bool,
    ) -> RS<Store<WasiContextComponent>> {
        control.check()?;
        let mut context = build_wasi_component_context(worker);
        context.set_invocation(control.clone());
        let mut store = Store::new(pre.engine(), context);
        store.limiter(|context| &mut context.memory);
        store
            .set_fuel(control.limits.fuel)
            .map_err(|e| mudu_error!(ErrorCode::Internal, "set procedure fuel", e))?;
        if asynchronous {
            store.fuel_async_yield_interval(Some(10_000)).map_err(|e| {
                mudu_error!(
                    ErrorCode::Internal,
                    "configure cooperative Wasm scheduling",
                    e
                )
            })?;
        }
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(move |_| {
            control.check()?;
            Ok(if asynchronous {
                wasmtime::UpdateDeadline::Yield(1)
            } else {
                wasmtime::UpdateDeadline::Continue(1)
            })
        });
        Ok(store)
    }

    /// Always zero: no request state is retained between invocations.
    #[cfg(test)]
    pub fn idle_len(&self, _name: &str) -> usize {
        0
    }
}

/// One invocation's isolated store. Dropping it destroys guest and WASI state.
pub struct LeasedInstance {
    inner: Option<PooledInstance>,
}
impl LeasedInstance {
    /// Binds the worker before invocation.
    pub fn set_worker_local(&mut self, worker: Option<WorkerLocalRef>) {
        if let Some(inner) = &mut self.inner {
            inner.store.data_mut().set_worker_local(worker);
        }
    }
    fn prepare(&mut self, param: ProcedureParam) -> RS<(PooledInstance, Vec<u8>)> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "instance consumed"))?;
        let control = inner.store.data().invocation();
        control.check()?;
        let bytes = procedure_invoke::serialize_param(param)?;
        if bytes.len() > control.limits.max_input_bytes {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "procedure input exceeds limit"
            ));
        }
        Ok((inner, bytes))
    }
    fn finish(inner: &PooledInstance, result: wasmtime::Result<(Vec<u8>,)>) -> RS<ProcedureResult> {
        let control = inner.store.data().invocation();
        control.check()?;
        let (bytes,) = result.map_err(|e| {
            mudu_error!(
                ErrorCode::DomainViolation,
                "procedure trapped or exhausted its budget",
                e
            )
        })?;
        if bytes.len() > control.limits.max_output_bytes {
            return Err(mudu_error!(
                ErrorCode::OutOfMemory,
                "procedure output exceeds limit"
            ));
        }
        procedure_invoke::deserialize_result(&bytes)
    }
    /// Executes and destroys this instance, including on trap or cancellation.
    pub async fn invoke(mut self, param: ProcedureParam) -> RS<ProcedureResult> {
        let (mut inner, bytes) = self.prepare(param)?;
        let result = inner
            .typed_func
            .call_async(&mut inner.store, (bytes,))
            .await;
        Self::finish(&inner, result)
    }
    /// Executes on a plain thread and destroys this instance.
    pub fn invoke_sync(mut self, param: ProcedureParam) -> RS<ProcedureResult> {
        let (mut inner, bytes) = self.prepare(param)?;
        let result = inner.typed_func.call(&mut inner.store, (bytes,));
        Self::finish(&inner, result)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unimplemented)]

    use super::LeasedInstance;
    use crate::procedure::procedure::Procedure;
    use crate::service::app_package::AppPackage;
    use crate::service::procedure_invoke_component::{ProcOpt, ProcedureInvokeComponent};
    use crate::service::runtime_opt::{ComponentTarget, RuntimeOpt};
    use crate::service::test_wasm_mod_path::wasm_mod_path;
    use crate::service::wasi_context_component::WasiContextComponent;
    use crate::service::wt_runtime_component::WTRuntimeComponent;
    use ::catalog::contract::message_bus_api::MessageBusRef;
    use ::catalog::contract::meta_mgr::MetaMgr;
    use ::catalog::contract::worker_snapshot::KvItem;
    use ::conn::worker_local::{WorkerExecute, WorkerLocal, WorkerLocalRef};
    use ::xengine::api::XContract;
    use async_trait::async_trait;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use mudu::utils::case_convert::to_kebab_case;
    use mudu_contract::database::result_set::ResultSetAsync;
    use mudu_contract::database::sql_params::SQLParams;
    use mudu_contract::database::sql_stmt::SQLStmt;
    use mudu_contract::procedure::procedure_param::ProcedureParam;
    use mudu_type::data_value::DataValue;
    use std::path::PathBuf;
    use std::sync::Arc;

    /// A `WorkerLocal` whose identity is all that matters: pool tests compare
    /// instances by `Arc::ptr_eq`; no method is ever called.
    struct DummyWorkerLocal;

    #[async_trait]
    impl WorkerLocal for DummyWorkerLocal {
        fn x_contract(&self) -> Arc<dyn XContract> {
            unimplemented!()
        }

        fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
            unimplemented!()
        }

        fn message_bus(&self) -> MessageBusRef {
            unimplemented!()
        }

        async fn open_async(&self) -> RS<OID> {
            unimplemented!()
        }

        async fn close_async(&self, _session_id: OID) -> RS<()> {
            unimplemented!()
        }

        async fn execute_async(&self, _session_id: OID, _instruction: WorkerExecute) -> RS<()> {
            unimplemented!()
        }

        async fn put_async(&self, _session_id: OID, _key: Vec<u8>, _value: Vec<u8>) -> RS<()> {
            unimplemented!()
        }

        async fn delete_async(&self, _session_id: OID, _key: &[u8]) -> RS<()> {
            unimplemented!()
        }

        async fn get_async(&self, _session_id: OID, _key: &[u8]) -> RS<Option<Vec<u8>>> {
            unimplemented!()
        }

        async fn range_async(
            &self,
            _session_id: OID,
            _start_key: &[u8],
            _end_key: &[u8],
        ) -> RS<Vec<KvItem>> {
            unimplemented!()
        }

        async fn query(
            &self,
            _oid: OID,
            _app_name: Option<&str>,
            _sql: Box<dyn SQLStmt>,
            _param: Box<dyn SQLParams>,
        ) -> RS<Arc<dyn ResultSetAsync>> {
            unimplemented!()
        }

        async fn execute(
            &self,
            _oid: OID,
            _app_name: Option<&str>,
            _sql: Box<dyn SQLStmt>,
            _param: Box<dyn SQLParams>,
        ) -> RS<u64> {
            unimplemented!()
        }

        async fn batch(
            &self,
            _oid: OID,
            _app_name: Option<&str>,
            _sql: Box<dyn SQLStmt>,
            _param: Box<dyn SQLParams>,
        ) -> RS<u64> {
            unimplemented!()
        }
    }

    fn get_procedure(proc_name: &str) -> Procedure {
        let package = AppPackage::load(PathBuf::from(wasm_mod_path()).join("app1.mpk")).unwrap();
        let mut runtime = WTRuntimeComponent::build(&RuntimeOpt {
            invocation_limits: Default::default(),
            component_target: ComponentTarget::P2,
            enable_async: true,
            sever_mode: Default::default(),
            async_runtime: None,
            defer_initdb: false,
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

    /// Successful invocations must not retain a live guest instance.
    #[tokio::test]
    #[cfg_attr(miri, ignore)]
    async fn completed_instances_are_not_retained() {
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
        assert_eq!(proc.instance().pool_idle_len(&func_name), 0);
    }

    fn func_name(proc: &Procedure) -> String {
        to_kebab_case(&format!(
            "{}{}",
            mudu_contract::procedure::proc::MUDU_PROC_P2_PREFIX,
            proc.proc_name()
        ))
    }

    fn store_addr(leased: &LeasedInstance) -> usize {
        let pooled = leased.inner.as_ref().expect("lease holds an instance");
        // Identify the store by its heap-allocated inner data: `PooledInstance`
        // is moved between pool and lease, so the address of the `Store`
        // wrapper field itself changes across a pool round trip while the
        // store's data allocation stays put.
        pooled.store.data() as *const WasiContextComponent as usize
    }

    fn lease_worker_local(leased: &LeasedInstance) -> Option<WorkerLocalRef> {
        leased
            .inner
            .as_ref()
            .expect("lease holds an instance")
            .store
            .data()
            .worker_local()
    }

    /// Pool exclusivity under concurrency: two leases checked out at the same
    /// time must hold two distinct stores (one in-flight call per Store), and
    /// each store keeps the worker-local binding its caller set — concurrent
    /// invocations on different instances cannot cross-contaminate.
    #[tokio::test]
    #[cfg_attr(miri, ignore)]
    async fn concurrent_leases_hold_distinct_stores_and_own_worker_locals() {
        let proc = get_procedure("proc2_mtp");
        let func_name = func_name(&proc);
        let instance_pre = proc.instance();

        let mut lease_a = instance_pre.lease(&func_name).await.unwrap();
        let mut lease_b = instance_pre.lease(&func_name).await.unwrap();
        assert_eq!(instance_pre.pool_idle_len(&func_name), 0);
        assert_ne!(
            store_addr(&lease_a),
            store_addr(&lease_b),
            "two live leases must never share a store"
        );

        let worker_a: WorkerLocalRef = Arc::new(DummyWorkerLocal);
        let worker_b: WorkerLocalRef = Arc::new(DummyWorkerLocal);
        lease_a.set_worker_local(Some(worker_a.clone()));
        lease_b.set_worker_local(Some(worker_b.clone()));
        assert!(Arc::ptr_eq(
            &lease_worker_local(&lease_a).unwrap(),
            &worker_a
        ));
        assert!(Arc::ptr_eq(
            &lease_worker_local(&lease_b).unwrap(),
            &worker_b
        ));

        // Both leases drop without invoking: the clean instances go back.
        drop(lease_a);
        drop(lease_b);
        assert_eq!(instance_pre.pool_idle_len(&func_name), 0);
    }

    /// Releasing a lease never leaves a previous worker binding in the next call.
    #[tokio::test]
    #[cfg_attr(miri, ignore)]
    async fn new_invocation_has_no_previous_worker_binding() {
        let proc = get_procedure("proc2_mtp");
        let func_name = func_name(&proc);
        let instance_pre = proc.instance();
        let mut first = instance_pre.lease(&func_name).await.unwrap();
        first.set_worker_local(Some(Arc::new(DummyWorkerLocal)));
        first.invoke(sample_param()).await.unwrap();
        assert_eq!(instance_pre.pool_idle_len(&func_name), 0);
        let second = instance_pre.lease(&func_name).await.unwrap();
        assert!(lease_worker_local(&second).is_none());
        let result = second.invoke(sample_param()).await.unwrap();
        assert_eq!(result.return_list()[0].to_i32(), 5);
    }
}

#[cfg(test)]
#[path = "procedure_instance_budget_test.rs"]
mod budget_tests;
