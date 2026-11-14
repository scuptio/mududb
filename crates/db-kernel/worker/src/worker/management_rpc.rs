//! The worker's management-plane channel service: queue activation control
//! and queue observability, served by the worker over the channel.
//!
//! The HTTP management layer runs on foreign (actix) threads; serving these
//! requests on the worker thread keeps thread affinity correct by mechanism
//! (procedure invocation and queue runtime loads are thread-confined) and
//! gives the management layer the channel's timeout/nack semantics.

use super::WorkerRuntime;
use mudu::common::id::{INVALID_OID, OID};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::protocol::ProcedureInvokeRequest;
use mudu_type::data_value::DataValue;
use mudu_utils::channel::{current_channel, ServiceRouter};
use std::sync::Arc;
use xcontract::{
    ManagementRequest, ManagementResponse, QueueActivationInvoker, QueueConsumerSnapshot,
};

impl WorkerRuntime {
    /// Registers this worker's management-plane service on its channel
    /// exactly once (later calls are no-ops).
    pub fn ensure_management_rpc_handler(&self) -> RS<()> {
        if self.management_service.lock()?.is_some() {
            return Ok(());
        }
        let worker = self.clone();
        let router =
            ServiceRouter::new().handle::<ManagementRequest, _, _>(move |request, _ctx| {
                let worker = worker.clone();
                async move { worker.handle_management_request(request).await }
            })?;
        let handle = current_channel()?.serve(router)?;
        *self.management_service.lock()? = Some(handle);
        Ok(())
    }

    /// Executes one management-plane request on this worker.
    async fn handle_management_request(
        &self,
        request: ManagementRequest,
    ) -> RS<ManagementResponse> {
        match request {
            ManagementRequest::QueueActivationRegister(registration) => {
                if !self.has_procedure_runtime() {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        "the worker owning the queue has no procedure runtime configured; queue activations require one"
                    ));
                }
                // An activation drives keyless receives; a sharded queue has
                // no single receive target, so activations stay
                // unpartitioned-only for now.
                let shards = self
                    .x_contract()
                    .queue_shards(registration.queue_oid)
                    .await?;
                if shards.len() > 1 {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        "queue activation on a partitioned queue is not supported yet"
                    ));
                }
                let invoker =
                    WorkerActivationInvoker::new(self.clone(), registration.procedure.clone())?;
                self.contract
                    .queue_activation_register(registration, Arc::new(invoker))?;
                Ok(ManagementResponse::Registered)
            }
            ManagementRequest::QueueActivationUnregister { queue_oid } => {
                Ok(ManagementResponse::Unregistered(
                    self.contract.queue_activation_unregister(queue_oid)?,
                ))
            }
            ManagementRequest::QueueActivationList => Ok(ManagementResponse::Activations(
                self.contract.queue_activation_list()?,
            )),
            ManagementRequest::QueueProbe { queue_oid } => {
                let worker_id = self.x_contract().local_worker_id();
                match self.x_contract().queue_shards(queue_oid).await {
                    Ok(shards) => {
                        let mut hosted = Vec::new();
                        for shard in shards {
                            if shard.worker_id == worker_id {
                                self.x_contract().queue_prepare_shard(&shard).await?;
                                hosted.push(shard.shard_index);
                            }
                        }
                        Ok(ManagementResponse::Probe { shards: hosted })
                    }
                    Err(err) if err.ec() == ErrorCode::EntityNotFound => {
                        Ok(ManagementResponse::Probe { shards: Vec::new() })
                    }
                    Err(err) => Err(err),
                }
            }
            ManagementRequest::QueueStats => {
                Ok(ManagementResponse::Stats(self.queue_runtime_snapshot()?))
            }
            ManagementRequest::QueueConsumerList { queue_oid } => {
                Ok(ManagementResponse::Consumers(
                    self.x_contract()
                        .consumer_list(queue_oid)
                        .await?
                        .into_iter()
                        .map(|entry| QueueConsumerSnapshot {
                            shard: entry.shard,
                            name: entry.name,
                            frontier: entry.frontier,
                        })
                        .collect(),
                ))
            }
        }
    }
}

/// The [`QueueActivationInvoker`] over the worker's procedure runtime: one
/// internal admin session per registration (created at construction; the
/// worker session registry has no cross-crate close, so the session lives as
/// long as the registration). The procedure receives the queue name as its
/// single `String` parameter.
struct WorkerActivationInvoker {
    worker: WorkerRuntime,
    /// Synthetic connection token owning the internal session.
    #[allow(dead_code)]
    conn_id: u64,
    session_id: OID,
    procedure: String,
}

impl WorkerActivationInvoker {
    fn new(worker: WorkerRuntime, procedure: String) -> RS<Self> {
        let conn_id = mudu_utils::oid::new_xid() as u64;
        let session_id = worker.create_session_with_admin(conn_id, true)?;
        Ok(Self {
            worker,
            conn_id,
            session_id,
            procedure,
        })
    }
}

#[async_trait::async_trait]
impl QueueActivationInvoker for WorkerActivationInvoker {
    async fn invoke(&self, queue_name: &str) -> RS<()> {
        let param = procedure_invoke::serialize_param(ProcedureParam::new(
            INVALID_OID,
            0,
            vec![DataValue::from_string(queue_name.to_string())],
        ))?;
        let response = self
            .worker
            .handle_procedure_request(
                self.conn_id,
                &ProcedureInvokeRequest::new(self.session_id, self.procedure.clone(), param),
            )
            .await?;
        // The procedure host reports guest failures inside the serialized
        // result rather than as the invoke error: unwrap and surface them as
        // invocation failures so the driver's backoff/disable logic applies.
        procedure_invoke::deserialize_result(&response.into_result())?;
        Ok(())
    }
}
