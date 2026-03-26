use super::{
    HttpApi, legacy_invoke_async_proc, legacy_invoke_sync_proc, parse_json_object_body,
    runtime_get_app_and_desc,
};
use crate::service::runtime::Runtime;
use async_trait::async_trait;
use mudu::common::id::gen_oid;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::json::JsonValue;
use mudu_contract::procedure::proc_desc::ProcDesc;
use serde_json::Value;
use std::env::temp_dir;
use std::fs;
use std::sync::Arc;

pub struct LegacyHttpApi {
    service: Arc<dyn Runtime>,
}

impl LegacyHttpApi {
    pub fn new(service: Arc<dyn Runtime>) -> Self {
        Self { service }
    }
}

#[async_trait(?Send)]
impl HttpApi for LegacyHttpApi {
    async fn list_apps(&self) -> RS<Vec<String>> {
        Ok(self.service.list().await)
    }

    async fn list_procedures(&self, app_name: &str) -> RS<Vec<String>> {
        let procedure_list = if let Some(app) = self.service.app(app_name.to_string()).await {
            app.procedure()?
        } else {
            Vec::new()
        };
        Ok(procedure_list
            .iter()
            .map(|e| format!("{}/{}", e.0, e.1))
            .collect())
    }

    async fn procedure_detail(
        &self,
        app_name: &str,
        mod_name: &str,
        proc_name: &str,
    ) -> RS<(ProcDesc, JsonValue, JsonValue)> {
        let app = self
            .service
            .app(app_name.to_string())
            .await
            .ok_or_else(|| {
                m_error!(
                    EC::NoneErr,
                    format!("procedure detail error, no such app {}", app_name)
                )
            })?;
        let desc = app.describe(&mod_name.to_string(), &proc_name.to_string())?;
        Ok((
            desc.as_ref().clone(),
            desc.default_param_json()?,
            desc.default_return_json()?,
        ))
    }

    async fn install_mpk(&self, mpk_binary: Vec<u8>) -> RS<()> {
        let temp_mpk_file = temp_dir().join(format!("{:x}.mpk", gen_oid()));
        fs::write(&temp_mpk_file, &mpk_binary)
            .map_err(|e| m_error!(EC::IOErr, "write temp mpk file error", e))?;
        let file_path = temp_mpk_file
            .as_path()
            .to_str()
            .ok_or_else(|| m_error!(EC::IOErr, "cannot get string of PathBuf"))?
            .to_string();
        self.service.install(file_path).await
    }

    async fn invoke_json(
        &self,
        app_name: &str,
        mod_name: &str,
        proc_name: &str,
        body: String,
    ) -> RS<Value> {
        let map = parse_json_object_body(&body)?;
        let (app, desc) =
            runtime_get_app_and_desc(self.service.clone(), app_name, mod_name, proc_name).await?;
        let result = if app.cfg().use_async {
            legacy_invoke_async_proc(mod_name, proc_name, map, app, desc).await?
        } else {
            legacy_invoke_sync_proc(mod_name, proc_name, map, app, desc).await??
        };
        Ok(result)
    }
}
