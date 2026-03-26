use super::{
    AsyncIoUringInvokeClientFactory, HttpApi, TokioIoUringInvokeClientFactory, find_app,
    parse_json_object_body, to_param,
};
use crate::backend::app_mgr::AppMgr;
use crate::backend::mududb_cfg::MuduDBCfg;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::utils::json::JsonValue;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::proc_desc::ProcDesc;
use serde_json::Value;
use std::sync::Arc;

pub struct IoUringHttpApi {
    app_mgr: Arc<dyn AppMgr>,
    tcp_addr: String,
    client_factory: Arc<dyn AsyncIoUringInvokeClientFactory>,
}

impl IoUringHttpApi {
    pub fn new(app_mgr: Arc<dyn AppMgr>, cfg: &MuduDBCfg) -> Self {
        Self::with_client_factory(
            app_mgr,
            format!("{}:{}", cfg.listen_ip, cfg.tcp_listen_port),
            Arc::new(TokioIoUringInvokeClientFactory),
        )
    }

    pub fn with_client_factory(
        app_mgr: Arc<dyn AppMgr>,
        tcp_addr: String,
        client_factory: Arc<dyn AsyncIoUringInvokeClientFactory>,
    ) -> Self {
        Self {
            app_mgr,
            tcp_addr,
            client_factory,
        }
    }
}

#[async_trait(?Send)]
impl HttpApi for IoUringHttpApi {
    async fn list_apps(&self) -> RS<Vec<String>> {
        let list = self
            .app_mgr
            .list(&crate::backend::mudu_app_mgr::ListOption::default())
            .await?;
        Ok(list.apps.into_iter().map(|app| app.info.name).collect())
    }

    async fn list_procedures(&self, app_name: &str) -> RS<Vec<String>> {
        let app = find_app(self.app_mgr.as_ref(), app_name).await?;
        Ok(app
            .mod_proc_desc
            .modules()
            .iter()
            .flat_map(|(mod_name, procedures)| {
                procedures
                    .iter()
                    .map(move |proc_desc| format!("{}/{}", mod_name, proc_desc.proc_name()))
            })
            .collect())
    }

    async fn procedure_detail(
        &self,
        app_name: &str,
        mod_name: &str,
        proc_name: &str,
    ) -> RS<(ProcDesc, JsonValue, JsonValue)> {
        let app = find_app(self.app_mgr.as_ref(), app_name).await?;
        let procedure = app
            .mod_proc_desc
            .modules()
            .get(mod_name)
            .and_then(|procedures| {
                procedures
                    .iter()
                    .find(|procedure| procedure.proc_name() == proc_name)
            })
            .cloned()
            .ok_or_else(|| {
                mudu::m_error!(
                    mudu::error::ec::EC::NoneErr,
                    format!("no such procedure {}/{}/{}", app_name, mod_name, proc_name)
                )
            })?;
        let param_json = procedure.default_param_json()?;
        let return_json = procedure.default_return_json()?;
        Ok((procedure, param_json, return_json))
    }

    async fn install_mpk(&self, mpk_binary: Vec<u8>) -> RS<()> {
        self.app_mgr.install(mpk_binary).await
    }

    async fn uninstall_app(&self, app_name: &str) -> RS<()> {
        self.app_mgr.uninstall(app_name.as_bytes().to_vec()).await
    }

    async fn invoke_json(
        &self,
        app_name: &str,
        mod_name: &str,
        proc_name: &str,
        body: String,
    ) -> RS<Value> {
        let map = parse_json_object_body(&body)?;
        let (desc, _, _) = self.procedure_detail(app_name, mod_name, proc_name).await?;
        let param = to_param(&map, desc.param_desc().fields())?;
        let payload = procedure_invoke::serialize_param(param)?;
        let procedure_name = format!("{}/{}/{}", app_name, mod_name, proc_name);
        let mut client = self.client_factory.connect(&self.tcp_addr).await?;
        let session_id = client.create_session(None).await?;
        let invoke_result = client
            .invoke_procedure(session_id, procedure_name, payload)
            .await;
        let close_result = client.close_session(session_id).await;
        let result_binary = match (invoke_result, close_result) {
            (Ok(binary), Ok(_)) => binary,
            (Err(invoke_err), _) => return Err(invoke_err),
            (Ok(_), Err(close_err)) => return Err(close_err),
        };
        let result = procedure_invoke::deserialize_result(&result_binary)?;
        procedure_invoke::result_to_json(result)
    }
}
