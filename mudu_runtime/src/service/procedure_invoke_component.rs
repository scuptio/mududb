#![allow(missing_docs)]

use crate::procedure::procedure::Procedure;
use crate::service::runtime_opt::ComponentTarget;
use crate::service::wasi_context_component::{WasiContextComponent, build_wasi_component_context};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_kebab_case;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use mudu_kernel::server::worker_local::WorkerLocalRef;
use mudu_sys::sync::SMutex;
use mudu_utils::task_trace;
use wasmtime::Store;
use wasmtime::component::{InstancePre, TypedFunc};

pub struct ProcedureInvokeComponent {
    inner: SMutex<ProcedureInvokeInner>,
}

impl ProcedureInvokeComponent {
    pub fn call(
        procedure: &Procedure,
        component_target: ComponentTarget,
        proc_opt: ProcOpt,
        param: ProcedureParam,
        worker_local: Option<WorkerLocalRef>,
    ) -> RS<ProcedureResult> {
        let name = component_proc_name(component_target, procedure.proc_name())?;
        let name = to_kebab_case(&name);
        let context = build_wasi_component_context(worker_local);
        let instance_pre = procedure.instance().as_component_instance_pre().clone();

        let thread = mudu_sys::task::sync::spawn_thread(move || {
            let runtime = mudu_sys::task::async_::build_current_thread_runtime().map_err(|e| {
                mudu_error!(ErrorCode::Internal, "build current thread runtime error", e)
            })?;
            runtime.block_on(async {
                let this: Self = Self::new_async(context, &instance_pre, name, proc_opt).await?;
                this.invoke_async(param).await
            })
        })
        .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn invoke thread error", e))?;

        thread
            .join()
            .map_err(|_e| mudu_error!(ErrorCode::Thread, "invoke thread join error"))?
    }

    pub async fn call_async(
        procedure: &Procedure,
        component_target: ComponentTarget,
        proc_opt: ProcOpt,
        param: ProcedureParam,
        worker_local: Option<WorkerLocalRef>,
    ) -> RS<ProcedureResult> {
        let trace = task_trace!();
        trace.watch("procedure.component.stage", "call_async_start");
        let name = component_proc_name(component_target, procedure.proc_name())?;
        let name = to_kebab_case(&name);
        trace.watch("procedure.component.name", &name);
        let context = build_wasi_component_context(worker_local);
        let p = procedure.instance().as_component_instance_pre();
        let this: Self = Self::new_async(context, p, name, proc_opt).await?;
        trace.watch("procedure.component.stage", "invoke_async_start");
        this.invoke_async(param).await
    }

    async fn new_async(
        context: WasiContextComponent,
        instance_pre: &InstancePre<WasiContextComponent>,
        name: String,
        proc_opt: ProcOpt,
    ) -> RS<Self> {
        Ok(Self {
            inner: SMutex::new(
                ProcedureInvokeInner::new_async(context, instance_pre, name, proc_opt).await?,
            ),
        })
    }

    async fn invoke_async(self, param: ProcedureParam) -> RS<ProcedureResult> {
        let inner = self.inner;
        let inner: ProcedureInvokeInner = inner
            .into_inner()
            .map_err(|e| mudu_error!(ErrorCode::Mutex, "mutex into inner", e))?;
        inner.invoke_async(param).await
    }
}

struct ProcedureInvokeInner {
    store: Store<WasiContextComponent>,
    typed_func: TypedFunc<(Vec<u8>,), (Vec<u8>,)>,
    _proc_opt: ProcOpt,
}

const PAGE_SIZE: u64 = 65536;

pub struct ProcOpt {
    pub memory: u64,
    pub async_call: bool,
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
    async fn new_async(
        context: WasiContextComponent,
        instance_pre: &InstancePre<WasiContextComponent>,
        name: String,
        proc_opt: ProcOpt,
    ) -> RS<ProcedureInvokeInner> {
        let mut store = Store::new(instance_pre.engine(), context);
        let instance = instance_pre
            .instantiate_async(&mut store)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Internal, "component instantiate error", e))?;
        let function = instance.get_func(&mut store, &name).map_or_else(
            || {
                Err(mudu_error!(
                    ErrorCode::Internal,
                    format!("no function named {}", name)
                ))
            },
            Ok,
        )?;
        let typed_function = function
            .typed::<(Vec<u8>,), (Vec<u8>,)>(&mut store)
            .map_err(|e| mudu_error!(ErrorCode::Internal, "get typed async function error", e))?;

        Ok(Self {
            store,
            typed_func: typed_function,
            _proc_opt: proc_opt,
        })
    }

    pub async fn invoke_async(self, param: ProcedureParam) -> RS<ProcedureResult> {
        let param_p2 = procedure_invoke::serialize_param(param)?;
        let mut store = self.store;
        let (result_binary,) = self
            .typed_func
            .call_async(&mut store, (param_p2,))
            .await
            .map_err(|e| mudu_error!(ErrorCode::DomainViolation, "invoke call async error", e))?;
        let result_p2 = procedure_invoke::deserialize_result(&result_binary)?;
        Ok(result_p2)
    }
}

fn component_proc_name(component_target: ComponentTarget, proc_name: &str) -> RS<String> {
    let prefix = match component_target {
        ComponentTarget::P2 => mudu_contract::procedure::proc::MUDU_PROC_P2_PREFIX,
        ComponentTarget::P3 => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "component target p3 is not implemented yet"
            ));
        }
    };
    Ok(format!("{}{}", prefix, proc_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procedure::procedure::Procedure;
    use crate::service::mudu_package::MuduPackage;
    use crate::service::runtime_opt::{ComponentTarget, RuntimeOpt};
    use crate::service::test_wasm_mod_path::wasm_mod_path;
    use crate::service::wt_runtime_component::WTRuntimeComponent;
    use mudu_contract::procedure::proc_desc::ProcDesc;
    use mudu_contract::procedure::procedure_param::ProcedureParam;
    use mudu_contract::tuple::tuple_datum::TupleDatum;
    use mudu_type::dat_value::DatValue;
    use std::path::PathBuf;

    fn app1_path() -> PathBuf {
        PathBuf::from(wasm_mod_path()).join("app1.mpk")
    }

    fn load_package() -> MuduPackage {
        MuduPackage::load(app1_path()).unwrap()
    }

    fn build_runtime(enable_async: bool) -> WTRuntimeComponent {
        let mut runtime = WTRuntimeComponent::build(&RuntimeOpt {
            component_target: ComponentTarget::P2,
            enable_async,
            sever_mode: Default::default(),
            async_runtime: None,
        })
        .unwrap();
        runtime.instantiate().unwrap();
        runtime
    }

    fn get_procedure(enable_async: bool, proc_name: &str) -> Procedure {
        let package = load_package();
        let modules = build_runtime(enable_async)
            .compile_modules(&package)
            .unwrap();
        let module = modules
            .into_iter()
            .find(|(name, _)| name == "mod_0")
            .unwrap()
            .1;
        module.procedure(proc_name).unwrap()
    }

    fn sample_param() -> ProcedureParam {
        ProcedureParam::new(
            0,
            0,
            vec![
                DatValue::from_i32(2),
                DatValue::from_i64(3),
                DatValue::from_string("hello".to_string()),
            ],
        )
    }

    #[test]
    fn component_proc_name_p2() {
        assert_eq!(
            component_proc_name(ComponentTarget::P2, "foo").unwrap(),
            "mp2_foo"
        );
    }

    #[test]
    fn component_proc_name_p3_returns_not_implemented() {
        let err = component_proc_name(ComponentTarget::P3, "foo").unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
        assert!(err.message().contains("not implemented yet"));
    }

    #[test]
    fn proc_opt_default_values() {
        let opt = ProcOpt::default();
        assert_eq!(opt.memory, 65536 * 2000);
        assert!(!opt.async_call);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn missing_function_name_returns_internal_error_containing_name() {
        let real_proc = get_procedure(false, "proc2_mtp");
        let fake_desc = ProcDesc::new(
            "mod_0".to_string(),
            "nonexistent_proc".to_string(),
            <()>::tuple_desc_static(&[]),
            <()>::tuple_desc_static(&[]),
            false,
        );
        let fake_proc = Procedure::new(fake_desc, real_proc.instance().clone());

        let result = ProcedureInvokeComponent::call(
            &fake_proc,
            ComponentTarget::P2,
            ProcOpt::default(),
            sample_param(),
            None,
        );

        let err = result.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Internal);
        assert!(err.message().contains("mp2-nonexistent"));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn p3_target_rejected_before_instantiation() {
        let proc = get_procedure(false, "proc2_mtp");
        let result = ProcedureInvokeComponent::call(
            &proc,
            ComponentTarget::P3,
            ProcOpt::default(),
            sample_param(),
            None,
        );
        let err = result.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore)]
    async fn async_call_real_procedure_returns_deserializable_result() {
        let proc = get_procedure(true, "proc2_mtp");
        let result = ProcedureInvokeComponent::call_async(
            &proc,
            ComponentTarget::P2,
            ProcOpt::default(),
            sample_param(),
            None,
        )
        .await;

        let result = result.expect("async call should return a deserializable result");
        assert_eq!(result.return_list().len(), 2);
        assert_eq!(result.return_list()[0].to_i32(), 5);
        let text = result.return_list()[1].as_string().expect("string return");
        assert!(text.contains("xid:"));
        assert!(text.contains("a=2"));
        assert!(text.contains("b=3"));
        assert!(text.contains("c=hello"));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn sync_call_runs_on_separate_thread_and_returns_successfully() {
        let proc = get_procedure(false, "proc2_mtp");
        let result = ProcedureInvokeComponent::call(
            &proc,
            ComponentTarget::P2,
            ProcOpt::default(),
            sample_param(),
            None,
        );

        let result = result.expect("sync call should succeed");
        assert_eq!(result.return_list().len(), 2);
        assert_eq!(result.return_list()[0].to_i32(), 5);
        let text = result.return_list()[1].as_string().expect("string return");
        assert!(text.contains("xid:"));
    }
}
