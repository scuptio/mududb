use crate::server::worker_local::WorkerLocalRef;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use std::sync::Arc;

#[async_trait]
pub trait AsyncFuncInvoker: Send + Sync {
    async fn invoke(
        &self,
        session_id: OID,
        procedure_name: &str,
        procedure_parameters: Vec<u8>,
        worker_local: WorkerLocalRef,
    ) -> RS<Vec<u8>>;
}

pub type AsyncFuncInvokerPtr = Arc<dyn AsyncFuncInvoker>;
