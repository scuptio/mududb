#[allow(unused)]
#[cfg(test)]
mod tests {
    use crate::service::runtime::Runtime;
    use crate::service::runtime_impl::create_runtime_service;
    use crate::service::runtime_opt::RuntimeOpt;
    use crate::service::test_wasm_mod_path::wasm_mod_path;
    use mudu::common::result::RS;
    use mudu::error::ec::EC;
    use mudu::m_error;
    use mudu_contract::procedure::procedure_param::ProcedureParam;
    use mudu_contract::tuple::tuple_datum::TupleDatum;
    use mudu_utils::log::log_setup_ex;
    use mudu_utils::notifier::NotifyWait;
    use mudu_utils::task::{spawn_task, this_task_id};
    use std::env::temp_dir;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[derive(Eq, PartialEq, Debug)]
    enum TestProc {
        Proc,
        ProcSysCall,
        ProvSysCallAsync,
    }
    ///
    /// See proc function definition [proc](mudu_wasm/src/wasm/proc.rs#L5)。
    ///
    //#[test]
    fn test_proc() {
        test_runtime_simple(TestProc::Proc)
    }

    ///
    /// See proc_sys_call function definition [proc_sys_call](mudu_wasm/src/wasm/proc2.rs#L11)。
    ///
    //#[test]
    fn test_proc_syscall() {
        test_runtime_simple(TestProc::ProcSysCall)
    }


    #[test]
    fn test_async() {
        test_runtime_simple(TestProc::ProvSysCallAsync)
    }
    fn test_runtime_simple(test_kind: TestProc) {
        log_setup_ex("debug", "", false);
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let r = test_async_runtime_simple(true, test_kind).await;
                println!("{:?}", r);
            });
    }

    fn db_path() -> String {
        let n = uuid::Uuid::new_v4().to_string();
        let path = PathBuf::from(temp_dir()).join(format!("test_runtime_service_{}", n));
        path.to_str().unwrap().to_string()
    }

    async fn test_async_runtime_simple(enable_p2: bool, test_kind: TestProc) -> RS<()> {
        let pkg_path = wasm_mod_path();
        let db_path = db_path();
        let enable_async = test_kind == TestProc::ProvSysCallAsync ||
            test_kind == TestProc::ProcSysCall;
        let service = create_runtime_service(&pkg_path, &db_path, None, RuntimeOpt { enable_p2, enable_async }).await?;

        let stopper = NotifyWait::new();
        let task = spawn_task(stopper.clone(), "test session task", async move {
            match test_kind {
                TestProc::Proc => {
                    async_session(service).await?;
                }
                TestProc::ProcSysCall => {
                    async_session_sys_call(service).await?;
                }
                TestProc::ProvSysCallAsync => {
                    async_session_sys_call_async(service).await?;
                }
            }
            Ok(())
        })?;
        let opt = task
            .await
            .map_err(|e| m_error!(EC::InternalErr, "join error", e))?;
        opt.unwrap_or_else(|| Ok(()))
    }


    async fn async_session(service: Arc<dyn Runtime>) -> RS<()> {
        println!("task id {}", this_task_id());
        let tuple = (1i32, 100i64, "string argument".to_string());
        let desc = <(i32, i64, String)>::tuple_desc_static(&[]);
        let param = ProcedureParam::from_tuple(0, tuple, &desc)?;
        let app_name = "app1".to_string();
        let app = service
            .app(app_name.clone()).await
            .ok_or_else(|| m_error!(EC::NoneErr, format!("no such app named {}", app_name)))?;
        let id = app.task_create().await?;
        let proc_result = app.invoke(id, &"mod_0".to_string(), &"proc".to_string(), param).await?;
        let result = proc_result.to::<(i32, String)>(&<(i32, String)>::tuple_desc_static(&[]))?;
        println!("result: {:?}", result);
        app.task_end(id)?;
        Ok(())
    }

    async fn async_session_sys_call_async(service: Arc<dyn Runtime>) -> RS<()> {
        println!("task id {}", this_task_id());
        let tuple = (1i32, 100i64, "string argument".to_string());
        let desc = <(i32, i64, String)>::tuple_desc_static(&[]);
        let param = ProcedureParam::from_tuple(0, tuple, &desc)?;
        let app_name = "app1".to_string();
        let app = service
            .app(app_name.clone()).await
            .ok_or_else(|| m_error!(EC::NoneErr, format!("no such app named {}", app_name)))?;
        let id = app.task_create().await?;
        let proc_result = app.invoke_async(id, &"mod_0".to_string(), &"proc_sys_call_mtp".to_string(), param).await?;
        let result = proc_result.to::<(i32, String)>(&<(i32, String)>::tuple_desc_static(&[]))?;
        println!("result: {:?}", result);
        app.task_end(id)?;
        Ok(())
    }

    #[allow(unused)]
    async fn async_session_sys_call(service: Arc<dyn Runtime>) -> RS<()> {
        println!("task id {}", this_task_id());
        let tuple = (1i32, 100i64, "string argument".to_string());
        let desc = <(i32, i64, String)>::tuple_desc_static(&[]);
        let param = ProcedureParam::from_tuple(0, tuple, &desc)?;
        let app_name = "app1".to_string();
        let app = service
            .app(app_name.clone()).await
            .ok_or_else(|| m_error!(EC::NoneErr, format!("no such app named {}", app_name)))?;
        let id = app.task_create().await?;
        let proc_result = app.invoke_async(id, &"mod_0".to_string(), &"proc_sys_call".to_string(), param).await?;
        let result = proc_result.to::<(i32, String)>(&<(i32, String)>::tuple_desc_static(&[]))?;
        println!("result: {:?}", result);
        app.task_end(id)?;
        Ok(())
    }
}