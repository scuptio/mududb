use crate::server::worker_local::WorkerLocal;
use crate::server::worker_registry::WorkerRegistry;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::protocol::{ProcedureInvokeRequest, ProcedureInvokeResponse};
use std::sync::Arc;

use crate::server::routing::SessionOpenConfig;

#[async_trait]
pub trait RequestResponseWorker: Send + Sync {
    fn worker_index(&self) -> usize;

    fn worker_id(&self) -> OID;

    fn registry(&self) -> Arc<WorkerRegistry>;

    fn open_session_with_config(&self, conn_id: u64, config: SessionOpenConfig) -> RS<OID>;

    fn close_session_for_connection(&self, conn_id: u64, session_id: OID) -> RS<bool>;

    async fn handle_procedure_request(
        &self,
        conn_id: u64,
        request: &ProcedureInvokeRequest,
    ) -> RS<ProcedureInvokeResponse>;
}

pub trait WorkerRuntimeApi: RequestResponseWorker + WorkerLocal {}

impl<T> WorkerRuntimeApi for T where T: RequestResponseWorker + WorkerLocal + ?Sized {}

pub type WorkerRuntimeRef = Arc<dyn WorkerRuntimeApi + Send + Sync>;
