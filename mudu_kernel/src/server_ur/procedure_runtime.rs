use crate::server_ur::worker_local::WorkerLocalRef;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use std::sync::Arc;

#[async_trait]
pub trait ProcInvoker: Send + Sync {
    // The kernel uses a binary procedure ABI so the TCP worker can forward a
    // decoded invoke request without depending on mudu_runtime internals.
    async fn invoke(
        &self,
        session_id: OID,
        procedure_name: &str,
        procedure_parameters: Vec<u8>,
        worker_local: WorkerLocalRef,
    ) -> RS<Vec<u8>>;
}

pub type ProcInvokerPtr = Arc<dyn ProcInvoker>;
