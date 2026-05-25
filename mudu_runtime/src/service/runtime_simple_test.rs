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
    use mudu_utils::task_async::spawn_task;
    use mudu_utils::task_trace::this_task_id;
    use std::env::temp_dir;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[derive(Eq, PartialEq, Debug)]
    enum TestProc {
        Proc,
        ProcSysCall,
        ProvSysCallAsync,
    }

    #[allow(dead_code)]
    fn test_proc() {
        test_runtime_simple(TestProc::Proc)
    }

    #[allow(dead_code)]
    fn test_proc_syscall() {
        test_runtime_simple(TestProc::ProcSysCall)
    }

    #[test]
    fn test_async() {
        test_runtime_simple(TestProc::ProvSysCallAsync)
    }

    fn test_runtime_simple(test_kind: TestProc) {
        log_setup_ex("info", "", false);
        mudu_sys::tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let r = test_async_runtime_simple(test_kind).await;
                println!("{:?}", r);
            });
    }

    fn db_path() -> String {
        let n = mudu_sys::random::next_uuid_v4_string();
        let path = PathBuf::from(temp_dir()).join(format!("test_runtime_service_{}", n));
        path.to_str().unwrap().to_string()
    }

    async fn test_async_runtime_simple(test_kind: TestProc) -> RS<()> {
        let pkg_path = wasm_mod_path();
        let db_path = db_path();
        let enable_async =
            test_kind == TestProc::ProvSysCallAsync || test_kind == TestProc::ProcSysCall;
        let service = create_runtime_service(
            &pkg_path,
            &db_path,
            None,
            RuntimeOpt {
                component_target: crate::service::runtime_opt::ComponentTarget::P2,
                enable_async,
                sever_mode: Default::default(),
                async_runtime: None,
            },
        )
        .await?;

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
            .app(app_name.clone())
            .await
            .ok_or_else(|| m_error!(EC::NoneErr, format!("no such app named {}", app_name)))?;
        let id = app.task_create().await?;
        let proc_result = app
            .invoke(id, &"mod_0".to_string(), &"proc".to_string(), param, None)
            .await?;
        let _result = proc_result.to::<(i32, String)>(&<(i32, String)>::tuple_desc_static(&[]))?;
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
            .app(app_name.clone())
            .await
            .ok_or_else(|| m_error!(EC::NoneErr, format!("no such app named {}", app_name)))?;
        let id = app.task_create().await?;
        let proc_result = app
            .invoke_async(
                id,
                &"mod_0".to_string(),
                &"proc_sys_call_mtp".to_string(),
                param,
                None,
            )
            .await?;
        let _result = proc_result.to::<(i32, String)>(&<(i32, String)>::tuple_desc_static(&[]))?;
        app.task_end(id)?;
        Ok(())
    }

    #[allow(dead_code)]
    async fn async_session_sys_call(service: Arc<dyn Runtime>) -> RS<()> {
        println!("task id {}", this_task_id());
        let tuple = (1i32, 100i64, "string argument".to_string());
        let desc = <(i32, i64, String)>::tuple_desc_static(&[]);
        let param = ProcedureParam::from_tuple(0, tuple, &desc)?;
        let app_name = "app1".to_string();
        let app = service
            .app(app_name.clone())
            .await
            .ok_or_else(|| m_error!(EC::NoneErr, format!("no such app named {}", app_name)))?;
        let id = app.task_create().await?;
        let proc_result = app
            .invoke_async(
                id,
                &"mod_0".to_string(),
                &"proc_sys_call".to_string(),
                param,
                None,
            )
            .await?;
        let _result = proc_result.to::<(i32, String)>(&<(i32, String)>::tuple_desc_static(&[]))?;
        app.task_end(id)?;
        Ok(())
    }
}
