use ::conn::worker_local::WorkerLocalRef;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use std::sync::Arc;

#[async_trait]
/// Invoker that runs a named procedure inside a session, implemented by the
/// server's procedure runtime (for example the WASM host).
pub trait AsyncFuncInvoker: Send + Sync {
    /// Invoke `procedure_name` in `session_id` with the serialized
    /// `procedure_parameters`, returning the serialized result.
    async fn invoke(
        &self,
        session_id: OID,
        procedure_name: &str,
        procedure_parameters: Vec<u8>,
        worker_local: WorkerLocalRef,
    ) -> RS<Vec<u8>>;
}

/// Shared handle to an [`AsyncFuncInvoker`].
pub type AsyncFuncInvokerPtr = Arc<dyn AsyncFuncInvoker>;
