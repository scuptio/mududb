use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::protocol::{
    encode_get_response, encode_procedure_invoke_response, encode_put_response,
    encode_range_scan_response, encode_server_response, encode_session_close_response,
    encode_session_create_response, GetResponse, KeyValue, ProcedureInvokeResponse, PutResponse,
    RangeScanResponse, ServerPerfDigest, ServerResponse, SessionCloseResponse,
    SessionCreateResponse,
};
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_sys::perf::TxnStage;
use mudu_sys::time::instant_now;
use std::sync::Arc;

use crate::server::async_func_task::HandleResult;
use crate::server::request_response_worker::WorkerRuntimeRef;
use crate::server::routing::parse_session_open_config;
use crate::server::routing::SessionOpenConfig;
use crate::server::worker_registry::WorkerRegistry;

#[derive(Clone)]
pub(in crate::server) struct RequestCtx {
    worker: WorkerRuntimeRef,
    conn_id: u64,
    request_id: u64,
}

impl RequestCtx {
    pub(in crate::server) fn new(worker: WorkerRuntimeRef, conn_id: u64, request_id: u64) -> Self {
        Self {
            worker,
            conn_id,
            request_id,
        }
    }

    pub(in crate::server) fn request_id(&self) -> u64 {
        self.request_id
    }

    pub(in crate::server) fn worker_index(&self) -> usize {
        self.worker.worker_index()
    }

    pub(in crate::server) fn worker_id(&self) -> OID {
        self.worker.worker_id()
    }

    pub(in crate::server) fn registry(&self) -> Arc<WorkerRegistry> {
        self.worker.registry()
    }

    pub(in crate::server) fn parse_session_open_config(
        &self,
        config_json: Option<&str>,
    ) -> RS<SessionOpenConfig> {
        parse_session_open_config(
            config_json,
            self.worker_index(),
            self.worker_id(),
            self.registry().as_ref(),
        )
    }

    pub(in crate::server) async fn get(&self, session_id: OID, key: &[u8]) -> RS<HandleResult> {
        let value = self.worker.get_async(session_id, key).await?;
        Ok(HandleResult::Response(encode_get_response(
            self.request_id,
            &GetResponse::new(value),
        )?))
    }

    pub(in crate::server) async fn put(
        &self,
        session_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> RS<HandleResult> {
        let trace = mudu_utils::task_trace!();
        trace.watch("put.stage", "request_ctx_put_start");
        trace.watch("put.session_id", &session_id.to_string());
        self.worker.put_async(session_id, key, value).await?;
        trace.watch("put.stage", "request_ctx_put_encode_response");
        Ok(HandleResult::Response(encode_put_response(
            self.request_id,
            &PutResponse::new(true),
        )?))
    }

    pub(in crate::server) async fn invoke_procedure(
        &self,
        request: mudu_contract::protocol::ProcedureInvokeRequest,
        perf_digest: Option<ServerPerfDigest>,
    ) -> RS<HandleResult> {
        let trace = mudu_utils::task_trace!();
        trace.watch("procedure.kernel.request_ctx.stage", "handle_request_start");
        trace.watch(
            "procedure.kernel.request_ctx.session_id",
            &request.session_id().to_string(),
        );
        trace.watch(
            "procedure.kernel.request_ctx.name",
            request.procedure_name(),
        );
        let exec_start = instant_now();
        let response = self
            .worker
            .handle_procedure_request(self.conn_id, &request)
            .await?;
        let exec_ns = exec_start.elapsed().as_nanos() as u64;
        trace.watch("procedure.kernel.request_ctx.stage", "handle_request_done");
        trace.watch(
            "procedure.kernel.request_ctx.stage",
            "encode_response_start",
        );
        let mut response = ProcedureInvokeResponse::new(response.into_result());
        if let Some(mut digest) = perf_digest {
            digest.set(TxnStage::ProcedureExec, exec_ns);
            response = response.with_server_perf_digest(digest);
        }
        Ok(HandleResult::Response(encode_procedure_invoke_response(
            self.request_id,
            &response,
        )?))
    }

    pub(in crate::server) async fn range_scan(
        &self,
        session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<HandleResult> {
        let items = self
            .worker
            .range_async(session_id, start_key, end_key)
            .await?;
        Ok(HandleResult::Response(encode_range_scan_response(
            self.request_id,
            &RangeScanResponse::new(
                items
                    .into_iter()
                    .map(|item| KeyValue::new(item.key, item.value))
                    .collect(),
            ),
        )?))
    }

    pub(in crate::server) async fn query(
        &self,
        oid: OID,
        _app_name: &str,
        sql: &str,
        perf_digest: Option<ServerPerfDigest>,
    ) -> RS<HandleResult> {
        let exec_start = instant_now();
        let response = self
            .worker
            .query(oid, Box::new(sql.to_string()), Box::new(()))
            .await?;
        let mut response = Self::query_response(response, perf_digest).await?;
        let exec_ns = exec_start.elapsed().as_nanos() as u64;
        let mut digest = response
            .server_perf_digest()
            .copied()
            .unwrap_or_else(|| ServerPerfDigest::new(0));
        digest.set(TxnStage::QueryExec, exec_ns);
        response = response.with_server_perf_digest(digest);
        self.encode_server_response(response)
    }

    pub(in crate::server) async fn execute_sql(
        &self,
        oid: OID,
        _app_name: &str,
        sql: &str,
        perf_digest: Option<ServerPerfDigest>,
    ) -> RS<HandleResult> {
        let exec_start = instant_now();
        let affected_rows = self
            .worker
            .execute(oid, Box::new(sql.to_string()), Box::new(()))
            .await?;
        let exec_ns = exec_start.elapsed().as_nanos() as u64;
        let mut response = ServerResponse::new(
            TupleFieldDesc::new(Vec::new()),
            Vec::new(),
            affected_rows,
            None,
        );
        if let Some(mut digest) = perf_digest {
            digest.set(TxnStage::CommandExec, exec_ns);
            response = response.with_server_perf_digest(digest);
        }
        self.encode_server_response(response)
    }

    pub(in crate::server) async fn batch(
        &self,
        oid: OID,
        app_name: &str,
        sql: &str,
    ) -> RS<HandleResult> {
        let _ = app_name;
        let affected_rows = self
            .worker
            .batch(oid, Box::new(sql.to_string()), Box::new(()))
            .await?;
        let response = ServerResponse::new(
            TupleFieldDesc::new(Vec::new()),
            Vec::new(),
            affected_rows,
            None,
        );
        self.encode_server_response(response)
    }

    pub(in crate::server) async fn session_create(
        &self,
        config: SessionOpenConfig,
    ) -> RS<HandleResult> {
        if config.target_worker_index() == self.worker.worker_index() {
            Ok(HandleResult::Response(encode_session_create_response(
                self.request_id,
                &SessionCreateResponse::new(
                    self.worker.open_session_with_config(self.conn_id, config)?,
                ),
            )?))
        } else {
            Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                format!(
                    "session create landed on worker index {} worker id {}, expected worker index {} worker id {}; reconnect to the target worker port",
                    self.worker.worker_index(),
                    self.worker.worker_id(),
                    config.target_worker_index(),
                    config.worker_id()
                )
            ))
        }
    }

    pub(in crate::server) async fn session_close(&self, session_id: OID) -> RS<HandleResult> {
        Ok(HandleResult::Response(encode_session_close_response(
            self.request_id,
            &SessionCloseResponse::new(
                self.worker
                    .close_session_for_connection(self.conn_id, session_id)?,
            ),
        )?))
    }

    fn encode_server_response(&self, response: ServerResponse) -> RS<HandleResult> {
        Ok(HandleResult::Response(encode_server_response(
            self.request_id,
            &response,
        )?))
    }

    async fn query_response(
        result_set: Arc<dyn ResultSetAsync>,
        perf_digest: Option<ServerPerfDigest>,
    ) -> RS<ServerResponse> {
        let desc = result_set.desc().clone();
        let mut rows = Vec::new();
        while let Some(row) = result_set.next().await? {
            if row.values().len() != desc.fields().len() {
                return Err(mudu_error!(
                    ErrorCode::FatalInternal,
                    "non consistent column number"
                ));
            }
            rows.push(row);
        }
        let mut response = ServerResponse::new(desc, rows, 0, None);
        if let Some(digest) = perf_digest {
            response = response.with_server_perf_digest(digest);
        }
        Ok(response)
    }
}
