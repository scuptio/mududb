use super::AsyncIoUringInvokeClient;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_cli::client::async_client::{
    AsyncClient as KernelAsyncIoUringTcpClient, AsyncClientImpl as KernelTokioIoUringTcpClient,
};
use mudu_contract::protocol::{ProcedureInvokeRequest, SessionCloseRequest, SessionCreateRequest};

pub(super) struct TokioIoUringInvokeClient {
    inner: KernelTokioIoUringTcpClient,
}

impl TokioIoUringInvokeClient {
    pub(super) async fn connect(addr: &str) -> RS<Self> {
        Ok(Self {
            inner: KernelTokioIoUringTcpClient::connect(addr).await?,
        })
    }
}

#[async_trait(?Send)]
impl AsyncIoUringInvokeClient for TokioIoUringInvokeClient {
    async fn create_session(&mut self, config_json: Option<String>) -> RS<u128> {
        Ok(self
            .inner
            .create_session(SessionCreateRequest::new(config_json))
            .await?
            .session_id())
    }

    async fn invoke_procedure(
        &mut self,
        session_id: u128,
        procedure_name: String,
        procedure_parameters: Vec<u8>,
    ) -> RS<Vec<u8>> {
        Ok(self
            .inner
            .invoke_procedure(ProcedureInvokeRequest::new(
                session_id,
                procedure_name,
                procedure_parameters,
            ))
            .await?
            .into_result())
    }

    async fn close_session(&mut self, session_id: u128) -> RS<bool> {
        Ok(self
            .inner
            .close_session(SessionCloseRequest::new(session_id))
            .await?
            .closed())
    }
}
