use crate::worker_registry::WorkerRegistry;
use ::conn::worker_local::WorkerLocal;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::protocol::{ProcedureInvokeRequest, ProcedureInvokeResponse};
use std::sync::Arc;

use crate::routing::SessionOpenConfig;

#[async_trait]
/// Request/response surface a worker exposes to per-connection request
/// handlers: connection-scoped session lifecycle and procedure invocation.
pub trait RequestResponseWorker: Send + Sync {
    /// Zero-based index of this worker within the server.
    fn worker_index(&self) -> usize;

    /// Unique id of this worker within the server.
    fn worker_id(&self) -> OID;

    /// The worker registry shared by every worker of the server.
    fn registry(&self) -> Arc<WorkerRegistry>;

    /// Open the session described by `config` for connection `conn_id`,
    /// returning the session id. Fails when the config targets a different
    /// worker than this one.
    fn open_session_with_config(&self, conn_id: u64, config: SessionOpenConfig) -> RS<OID>;

    /// Close `session_id` on behalf of connection `conn_id`; returns `false`
    /// when the session does not exist and fails when another connection
    /// owns it.
    fn close_session_for_connection(&self, conn_id: u64, session_id: OID) -> RS<bool>;

    /// Handle one procedure-invoke request arriving on connection `conn_id`.
    async fn handle_procedure_request(
        &self,
        conn_id: u64,
        request: &ProcedureInvokeRequest,
    ) -> RS<ProcedureInvokeResponse>;
}

/// A worker that is both a request/response endpoint and a [`WorkerLocal`]
/// execution context.
pub trait WorkerRuntimeApi: RequestResponseWorker + WorkerLocal {}

impl<T> WorkerRuntimeApi for T where T: RequestResponseWorker + WorkerLocal + ?Sized {}

/// Shared handle to a worker through its combined runtime API.
pub type WorkerRuntimeRef = Arc<dyn WorkerRuntimeApi + Send + Sync>;
