#![allow(missing_docs)]

use super::{
    HttpApi, legacy_invoke_async_proc, legacy_invoke_sync_proc, parse_json_object_body,
    runtime_get_app_and_desc,
};
use crate::service::runtime::Runtime;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::json::JsonValue;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_utils::oid::gen_oid;
use serde_json::Value;
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
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("procedure detail error, no such app {}", app_name)
                )
            })?;
        let desc = app.describe(mod_name, proc_name)?;
        Ok((
            desc.as_ref().clone(),
            desc.default_param_json()?,
            desc.default_return_json()?,
        ))
    }

    async fn install_mpk(&self, mpk_binary: Vec<u8>) -> RS<()> {
        let service = self.service.clone();
        let file_path = actix_web::web::block(move || {
            let temp_mpk_file = mudu_sys::env_var::temp_dir().join(format!("{:x}.mpk", gen_oid()));
            mudu_sys::fs::sync::write(&temp_mpk_file, &mpk_binary)?;
            let file_path = temp_mpk_file
                .as_path()
                .to_str()
                .ok_or_else(|| mudu_error!(ErrorCode::InvalidUtf8, "cannot get string of PathBuf"))?
                .to_string();
            RS::Ok(file_path)
        })
        .await
        .map_err(|e| mudu_error!(ErrorCode::Thread, "blocking install task failed", e))??;
        service.install(file_path).await
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
