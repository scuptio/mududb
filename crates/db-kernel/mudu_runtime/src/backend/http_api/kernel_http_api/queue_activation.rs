use super::super::{QueueActivationRequest, ServerQueueActivation, ServerQueueActivationsResponse};
use super::KernelHttpApi;
use ::worker::worker::WorkerRuntime;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::channel::{Channel, channel_for};
use std::sync::Arc;
use std::time::Duration;
use xcontract::management_rpc::{ManagementRequest, ManagementResponse};
use xcontract::queue_activation::{
    QUEUE_ACTIVATION_BATCH_DEFAULT, QUEUE_ACTIVATION_MAX_RETRY_DEFAULT,
    QueueActivationRegistration, QueueActivationStatus,
};

/// Timeout of one management-plane ask.
const MANAGEMENT_RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// Queue activation management endpoints (`POST/DELETE
/// /mudu/server/queue_activation`, `GET /mudu/server/queue_activations`).
///
/// Registrations are volatile: they live in the memory of the worker owning
/// the queue and are lost on restart (the RFC's catalog-registered `ALTER
/// QUEUE ... SET ACTIVATION` form is out of P1 scope).
///
/// All worker-bound operations go through the channel: the request executes
/// on the owning worker's thread (thread affinity by mechanism, not by
/// convention), with the channel's timeout and nack semantics.
impl KernelHttpApi {
    /// The typed channel view of this server's management endpoint: in
    /// cluster mode the node endpoint (its derived id is topology-known, so
    /// cross-node asks route their responses back over the cluster
    /// transport); otherwise the node-local management gateway.
    pub(super) fn management_channel(&self) -> RS<Channel> {
        let server_instance_id = self.server_instance_id.ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "queue activations are not available on this node"
            )
        })?;
        let gateway = server::MANAGEMENT_GATEWAY_ENDPOINT_ID;
        let endpoint_id =
            match ::server::server::cluster_directory::cluster_directory_for(server_instance_id) {
                Some(directory) => {
                    let node_endpoint = catalog::contract::derive_oid::derive_node_endpoint(
                        directory.self_node_id(),
                    );
                    // The node endpoint registers when the cluster control
                    // plane starts; before that, the gateway still serves.
                    match channel_for(server_instance_id, node_endpoint) {
                        Ok(_) => node_endpoint,
                        Err(_) => gateway,
                    }
                }
                None => gateway,
            };
        Ok(Channel::new(channel_for(server_instance_id, endpoint_id)?))
    }

    /// Asks worker `worker_id` one management request and awaits its reply.
    pub(super) async fn ask_worker(
        &self,
        worker_id: OID,
        request: ManagementRequest,
    ) -> RS<ManagementResponse> {
        self.management_channel()?
            .endpoint::<ManagementRequest>(worker_id)
            .ask(request, MANAGEMENT_RPC_TIMEOUT)
            .await
    }

    /// Finds the worker hosting the queue `queue_oid`'s shard 0: activations
    /// live on unpartitioned queues, so the worker answering the probe with
    /// shard 0 among its hosted shards is the owner; non-owners decline.
    pub(super) async fn find_queue_owner(
        &self,
        workers: &[(Arc<WorkerRuntime>, u32)],
        queue_oid: OID,
    ) -> RS<OID> {
        for (worker, _) in workers {
            match self
                .ask_worker(
                    worker.worker_id(),
                    ManagementRequest::QueueProbe { queue_oid },
                )
                .await?
            {
                ManagementResponse::Probe { shards } if shards.contains(&0) => {
                    return Ok(worker.worker_id());
                }
                ManagementResponse::Probe { .. } => continue,
                other => {
                    return Err(mudu_error!(
                        ErrorCode::Internal,
                        format!("unexpected management reply for queue probe: {other:?}")
                    ));
                }
            }
        }
        Err(mudu_error!(
            ErrorCode::InvalidState,
            format!("queue {queue_oid} has no owner among the live workers")
        ))
    }

    /// Registers (or replaces) the activation of the queue named by
    /// `request.queue` on the worker owning it. Fails when the queue does not
    /// resolve to exactly one queue, when no live worker owns it, or when the
    /// owning worker cannot serve it (e.g. no procedure runtime).
    pub(super) async fn queue_activation_register(
        &self,
        request: QueueActivationRequest,
    ) -> RS<()> {
        let queue = self.resolve_queue_by_name(&request.queue).await?;
        let workers = self.live_workers()?;
        let owner_id = self.find_queue_owner(&workers, queue.id()).await?;
        let response = self
            .ask_worker(
                owner_id,
                ManagementRequest::QueueActivationRegister(QueueActivationRegistration {
                    queue_oid: queue.id(),
                    queue_name: queue.name().to_string(),
                    procedure: request.procedure,
                    max_retry: request
                        .max_retry
                        .unwrap_or(QUEUE_ACTIVATION_MAX_RETRY_DEFAULT),
                    batch: request.batch.unwrap_or(QUEUE_ACTIVATION_BATCH_DEFAULT),
                }),
            )
            .await?;
        match response {
            ManagementResponse::Registered => Ok(()),
            other => Err(mudu_error!(
                ErrorCode::Internal,
                format!("unexpected management reply for activation register: {other:?}")
            )),
        }
    }

    /// Removes the activation of the queue `queue` from whichever worker
    /// holds it. Fails with `ObjectNotFound` when the queue or the activation
    /// does not exist.
    pub(super) async fn queue_activation_unregister(&self, queue: String) -> RS<()> {
        let desc = self.resolve_queue_by_name(&queue).await?;
        let mut removed = false;
        for (worker, _) in self.live_workers()? {
            if let ManagementResponse::Unregistered(true) = self
                .ask_worker(
                    worker.worker_id(),
                    ManagementRequest::QueueActivationUnregister {
                        queue_oid: desc.id(),
                    },
                )
                .await?
            {
                removed = true;
            }
        }
        if !removed {
            return Err(mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("queue {} has no registered activation", desc.name())
            ));
        }
        Ok(())
    }

    /// Lists the activations registered on this server's workers.
    pub(super) async fn server_queue_activations(&self) -> RS<ServerQueueActivationsResponse> {
        let mut activations = Vec::new();
        for (worker, _) in self.live_workers()? {
            match self
                .ask_worker(worker.worker_id(), ManagementRequest::QueueActivationList)
                .await?
            {
                ManagementResponse::Activations(worker_activations) => {
                    for status in worker_activations {
                        activations.push(activation_status_to_dto(status));
                    }
                }
                other => {
                    return Err(mudu_error!(
                        ErrorCode::Internal,
                        format!("unexpected management reply for activation list: {other:?}")
                    ));
                }
            }
        }
        if activations.is_empty() {
            return Ok(ServerQueueActivationsResponse {
                mode: "none".to_string(),
                activations,
            });
        }
        Ok(ServerQueueActivationsResponse {
            mode: "ok".to_string(),
            activations,
        })
    }

    /// Resolves a queue reference: a `schema.name`-qualified name hits
    /// exactly; a bare name must match exactly one queue across the catalog.
    pub(super) async fn resolve_queue_by_name(
        &self,
        queue: &str,
    ) -> RS<Arc<::catalog::contract::table_desc::TableDesc>> {
        if let Some((schema, name)) = queue.split_once('.') {
            return self.meta_mgr.get_queue(schema, name).await?.ok_or_else(|| {
                mudu_error!(ErrorCode::ObjectNotFound, format!("no such queue {queue}"))
            });
        }
        let matches: Vec<_> = self
            .meta_mgr
            .list_queues()
            .await?
            .into_iter()
            .filter(|schema| schema.class_name() == queue)
            .collect();
        match matches.len() {
            1 => self
                .meta_mgr
                .get_queue(matches[0].schema(), matches[0].class_name())
                .await?
                .ok_or_else(|| {
                    mudu_error!(ErrorCode::ObjectNotFound, format!("no such queue {queue}"))
                }),
            0 => Err(mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("no such queue {queue}")
            )),
            _ => Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "queue name {queue} is ambiguous across schemas; qualify it as schema.name"
                )
            )),
        }
    }

    pub(super) fn live_workers(&self) -> RS<Vec<(Arc<WorkerRuntime>, u32)>> {
        let server_instance_id = self.server_instance_id.ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "queue activations are not available on this node"
            )
        })?;
        let workers = ::backup::backup::backup_workers(server_instance_id)?;
        if workers.is_empty() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                format!("server {server_instance_id} has no registered workers")
            ));
        }
        Ok(workers)
    }
}

/// Maps the xcontract activation status to the endpoint DTO.
pub(super) fn activation_status_to_dto(status: QueueActivationStatus) -> ServerQueueActivation {
    ServerQueueActivation {
        queue: status.queue_name,
        procedure: status.procedure,
        enabled: status.enabled,
        consecutive_failures: status.consecutive_failures,
        max_retry: status.max_retry,
        batch: status.batch,
    }
}
