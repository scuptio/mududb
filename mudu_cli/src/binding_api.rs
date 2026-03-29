use crate::client::async_client::{AsyncClient, AsyncClientImpl};
use crate::management::fetch_server_topology;
use base64::Engine;
use mudu_contract::protocol::{
    ClientRequest, GetRequest, ProcedureInvokeRequest, PutRequest, RangeScanRequest,
    SessionCloseRequest, SessionCreateRequest,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::Mutex;
use thiserror::Error;
use tokio::runtime::{Builder, Runtime};

#[derive(Debug, Error, uniffi::Error)]
pub enum MuduCliBindingError {
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MuduRow {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MuduServerResponseBinding {
    pub columns: Vec<String>,
    pub rows: Vec<MuduRow>,
    pub affected_rows: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MuduKeyValueBinding {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct WorkerTopologyBinding {
    pub worker_index: u64,
    pub worker_id: String,
    pub partitions: Vec<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ServerTopologyBinding {
    pub worker_count: u64,
    pub workers: Vec<WorkerTopologyBinding>,
}

#[derive(uniffi::Object)]
pub struct MuduTcpClient {
    runtime: Mutex<Runtime>,
    inner: Mutex<AsyncClientImpl>,
}

#[uniffi::export]
impl MuduTcpClient {
    #[uniffi::constructor]
    pub fn connect(addr: String) -> Result<Arc<Self>, MuduCliBindingError> {
        let runtime = new_runtime()?;
        let inner = runtime
            .block_on(AsyncClientImpl::connect(&addr))
            .map_err(binding_error)?;
        Ok(Arc::new(Self {
            runtime: Mutex::new(runtime),
            inner: Mutex::new(inner),
        }))
    }

    pub fn query(
        &self,
        app_name: String,
        sql: String,
    ) -> Result<MuduServerResponseBinding, MuduCliBindingError> {
        let runtime = self.runtime.lock().map_err(lock_error)?;
        let mut client = self.inner.lock().map_err(lock_error)?;
        let response = runtime
            .block_on(client.query(ClientRequest::new(app_name, sql)))
            .map_err(binding_error)?;
        Ok(convert_server_response(response))
    }

    pub fn execute(
        &self,
        app_name: String,
        sql: String,
    ) -> Result<MuduServerResponseBinding, MuduCliBindingError> {
        let runtime = self.runtime.lock().map_err(lock_error)?;
        let mut client = self.inner.lock().map_err(lock_error)?;
        let response = runtime
            .block_on(client.execute(ClientRequest::new(app_name, sql)))
            .map_err(binding_error)?;
        Ok(convert_server_response(response))
    }

    pub fn create_session(
        &self,
        config_json: Option<String>,
    ) -> Result<String, MuduCliBindingError> {
        let runtime = self.runtime.lock().map_err(lock_error)?;
        let mut client = self.inner.lock().map_err(lock_error)?;
        let session_id = runtime
            .block_on(client.create_session(SessionCreateRequest::new(config_json)))
            .map_err(binding_error)?
            .session_id();
        Ok(session_id.to_string())
    }

    pub fn close_session(
        &self,
        session_id: String,
    ) -> Result<bool, MuduCliBindingError> {
        let runtime = self.runtime.lock().map_err(lock_error)?;
        let mut client = self.inner.lock().map_err(lock_error)?;
        let closed = runtime
            .block_on(client.close_session(SessionCloseRequest::new(parse_session_id(&session_id)?)))
            .map_err(binding_error)?;
        Ok(closed.closed())
    }

    pub fn get(
        &self,
        session_id: String,
        key: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, MuduCliBindingError> {
        let runtime = self.runtime.lock().map_err(lock_error)?;
        let mut client = self.inner.lock().map_err(lock_error)?;
        let response = runtime
            .block_on(client.get(GetRequest::new(parse_session_id(&session_id)?, key)))
            .map_err(binding_error)?;
        Ok(response.into_value())
    }

    pub fn put(
        &self,
        session_id: String,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<bool, MuduCliBindingError> {
        let runtime = self.runtime.lock().map_err(lock_error)?;
        let mut client = self.inner.lock().map_err(lock_error)?;
        let response = runtime
            .block_on(client.put(PutRequest::new(parse_session_id(&session_id)?, key, value)))
            .map_err(binding_error)?;
        Ok(response.ok())
    }

    pub fn range_scan(
        &self,
        session_id: String,
        start_key: Vec<u8>,
        end_key: Vec<u8>,
    ) -> Result<Vec<MuduKeyValueBinding>, MuduCliBindingError> {
        let runtime = self.runtime.lock().map_err(lock_error)?;
        let mut client = self.inner.lock().map_err(lock_error)?;
        let response = runtime
            .block_on(client.range_scan(RangeScanRequest::new(
                parse_session_id(&session_id)?,
                start_key,
                end_key,
            )))
            .map_err(binding_error)?;
        Ok(response
            .into_items()
            .into_iter()
            .map(|item| MuduKeyValueBinding {
                key: item.key().to_vec(),
                value: item.value().to_vec(),
            })
            .collect())
    }

    pub fn invoke_procedure(
        &self,
        session_id: String,
        procedure_name: String,
        procedure_parameters: Vec<u8>,
    ) -> Result<Vec<u8>, MuduCliBindingError> {
        let runtime = self.runtime.lock().map_err(lock_error)?;
        let mut client = self.inner.lock().map_err(lock_error)?;
        let response = runtime
            .block_on(client.invoke_procedure(ProcedureInvokeRequest::new(
                parse_session_id(&session_id)?,
                procedure_name,
                procedure_parameters,
            )))
            .map_err(binding_error)?;
        Ok(response.into_result())
    }
}

#[uniffi::export]
pub fn fetch_server_topology_binding(
    http_addr: String,
) -> Result<ServerTopologyBinding, MuduCliBindingError> {
    let runtime = new_runtime()?;
    let topology = runtime
        .block_on(fetch_server_topology(&http_addr))
        .map_err(MuduCliBindingError::Message)?;
    Ok(ServerTopologyBinding {
        worker_count: topology.worker_count as u64,
        workers: topology
            .workers
            .into_iter()
            .map(|worker| WorkerTopologyBinding {
                worker_index: worker.worker_index as u64,
                worker_id: worker.worker_id.to_string(),
                partitions: worker
                    .partitions
                    .into_iter()
                    .map(|partition| partition.to_string())
                    .collect(),
            })
            .collect(),
    })
}

#[uniffi::export]
pub fn install_app_package(
    http_addr: String,
    mpk_binary: Vec<u8>,
) -> Result<bool, MuduCliBindingError> {
    let runtime = new_runtime()?;
    let payload = json!({
        "mpk_base64": base64::engine::general_purpose::STANDARD.encode(mpk_binary),
    });
    let _ = runtime.block_on(post_http_json(&http_addr, "/mudu/app/install", payload))?;
    Ok(true)
}

fn convert_server_response(response: mudu_contract::protocol::ServerResponse) -> MuduServerResponseBinding {
    MuduServerResponseBinding {
        columns: response.columns().to_vec(),
        rows: response
            .rows()
            .iter()
            .cloned()
            .map(|values| MuduRow { values })
            .collect(),
        affected_rows: response.affected_rows(),
        error: response.error().map(|value| value.to_string()),
    }
}

fn parse_session_id(session_id: &str) -> Result<u128, MuduCliBindingError> {
    session_id.parse::<u128>().map_err(|e| {
        MuduCliBindingError::Message(format!(
            "invalid session_id '{}': {}",
            session_id, e
        ))
    })
}

fn binding_error(err: impl ToString) -> MuduCliBindingError {
    MuduCliBindingError::Message(err.to_string())
}

fn lock_error<T>(_err: std::sync::PoisonError<T>) -> MuduCliBindingError {
    MuduCliBindingError::Message("client mutex poisoned".to_string())
}

fn new_runtime() -> Result<Runtime, MuduCliBindingError> {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(binding_error)
}

async fn post_http_json(
    http_addr: &str,
    path: &str,
    payload: Value,
) -> Result<Value, MuduCliBindingError> {
    let url = format!("http://{}{}", http_addr, path);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(binding_error)?;
    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(binding_error)?;
    let body = response.json::<Value>().await.map_err(binding_error)?;
    let status = body
        .get("status")
        .and_then(Value::as_i64)
        .ok_or_else(|| MuduCliBindingError::Message("HTTP API response missing numeric status".to_string()))?;
    if status == 0 {
        return Ok(body.get("data").cloned().unwrap_or(Value::Null));
    }
    let message = body
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("HTTP API request failed");
    Err(MuduCliBindingError::Message(format!(
        "{}: {}",
        message,
        body.get("data").cloned().unwrap_or(Value::Null)
    )))
}
