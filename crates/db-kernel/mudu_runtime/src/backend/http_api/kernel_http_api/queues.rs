use super::super::{
    ServerQueueActivation, ServerQueueConsumer, ServerQueueShardStatus, ServerQueueStatus,
    ServerQueuesResponse,
};
use super::KernelHttpApi;
use super::queue_activation::activation_status_to_dto;
use ::catalog::contract::queue::queue_dlq_name;
use ::catalog::contract::schema_class::SchemaClass;
use ::worker::worker::WorkerRuntime;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use xcontract::management_rpc::{ManagementRequest, ManagementResponse, QueueConsumerSnapshot};

/// Queue observability endpoint (`GET /mudu/server/queues`).
impl KernelHttpApi {
    /// Reads the per-queue runtime stats: the catalog lists every queue
    /// (including the derived `<name>_dlq` dead-letter queues, which are
    /// folded into their owning queue's `dlq_depth`), and the live workers
    /// contribute the in-memory counters, the parked-waiter counts, and the
    /// named consumer cursors (fan-out mode). A sharded queue reports one
    /// `shards` entry per shard hosted on each worker.
    pub(super) async fn server_queues(&self) -> RS<ServerQueuesResponse> {
        let schemas = self.meta_mgr.list_queues().await?;
        if schemas.is_empty() {
            // Explicit empty state, in the style of the replication
            // endpoint's `mode: "none"`.
            return Ok(ServerQueuesResponse {
                mode: "none".to_string(),
                queues: Vec::new(),
            });
        }
        let server_instance_id = self.server_instance_id.ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "queue runtime stats are not available on this node"
            )
        })?;
        let workers = ::backup::backup::backup_workers(server_instance_id)?;
        if workers.is_empty() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                format!("server {server_instance_id} has no registered workers")
            ));
        }
        let consumers = self.collect_consumer_lists(&workers, &schemas).await?;
        let stats = self.collect_queue_stats(&workers).await?;
        let activations = self.collect_activations(&workers).await?;
        self.assemble_queue_status(&schemas, &stats, &consumers, &activations)
            .await
    }

    /// Collects the named consumer cursors of every queue from the workers
    /// hosting its shards. Every worker answers the probe with the shards it
    /// hosts and serves the cursor list of its own shards (the cursor cache
    /// lives with the shard runtime). All of it runs on the owning workers'
    /// threads through the channel.
    async fn collect_consumer_lists(
        &self,
        workers: &[(Arc<WorkerRuntime>, u32)],
        schemas: &[SchemaClass],
    ) -> RS<HashMap<OID, Vec<QueueConsumerSnapshot>>> {
        let mut consumers: HashMap<OID, Vec<QueueConsumerSnapshot>> = HashMap::new();
        for schema in schemas {
            for (worker, _) in workers {
                let probe = self
                    .ask_worker(
                        worker.worker_id(),
                        ManagementRequest::QueueProbe {
                            queue_oid: schema.id(),
                        },
                    )
                    .await?;
                match probe {
                    ManagementResponse::Probe { shards } if !shards.is_empty() => {
                        let mut list = self.consumer_list_of(worker, schema).await?;
                        consumers.entry(schema.id()).or_default().append(&mut list);
                    }
                    ManagementResponse::Probe { .. } => continue,
                    other => return Err(unexpected_reply("queue probe", other)),
                }
            }
        }
        Ok(consumers)
    }

    /// The named consumer cursor list of one queue's shards hosted on one
    /// worker.
    async fn consumer_list_of(
        &self,
        worker: &Arc<WorkerRuntime>,
        schema: &SchemaClass,
    ) -> RS<Vec<QueueConsumerSnapshot>> {
        match self
            .ask_worker(
                worker.worker_id(),
                ManagementRequest::QueueConsumerList {
                    queue_oid: schema.id(),
                },
            )
            .await?
        {
            ManagementResponse::Consumers(list) => Ok(list),
            other => Err(unexpected_reply("consumer list", other)),
        }
    }

    /// Per-shard runtime stats of every live worker, grouped by queue table
    /// OID.
    async fn collect_queue_stats(
        &self,
        workers: &[(Arc<WorkerRuntime>, u32)],
    ) -> RS<HashMap<OID, Vec<(u32, ::xcontract::queue_types::QueueRuntimeSnapshot)>>> {
        let mut stats: HashMap<OID, Vec<(u32, ::xcontract::queue_types::QueueRuntimeSnapshot)>> =
            HashMap::new();
        for (worker, worker_index) in workers {
            match self
                .ask_worker(worker.worker_id(), ManagementRequest::QueueStats)
                .await?
            {
                ManagementResponse::Stats(worker_stats) => {
                    for snapshot in worker_stats {
                        stats
                            .entry(snapshot.table_id())
                            .or_default()
                            .push((*worker_index, snapshot));
                    }
                }
                other => return Err(unexpected_reply("queue stats", other)),
            }
        }
        Ok(stats)
    }

    /// Activation registrations of every live worker (empty on non-owners),
    /// keyed by queue table OID.
    async fn collect_activations(
        &self,
        workers: &[(Arc<WorkerRuntime>, u32)],
    ) -> RS<HashMap<OID, ServerQueueActivation>> {
        let mut activations = HashMap::new();
        for (worker, _) in workers {
            match self
                .ask_worker(worker.worker_id(), ManagementRequest::QueueActivationList)
                .await?
            {
                ManagementResponse::Activations(worker_activations) => {
                    for status in worker_activations {
                        activations.insert(status.queue_oid, activation_status_to_dto(status));
                    }
                }
                other => return Err(unexpected_reply("activation list", other)),
            }
        }
        Ok(activations)
    }

    /// Joins the queue catalog entries with the per-worker shard stats, the
    /// consumer cursor lists, and the activation registrations into the
    /// response. `stats` maps a queue table OID to its shard snapshots with
    /// hosting worker indexes; `consumers` maps a queue table OID to its
    /// per-shard cursor entries (absent or empty while the queue is in
    /// competing mode); `activations` maps a queue table OID to its
    /// activation registration (absent while the queue has none).
    async fn assemble_queue_status(
        &self,
        schemas: &[SchemaClass],
        stats: &HashMap<OID, Vec<(u32, ::xcontract::queue_types::QueueRuntimeSnapshot)>>,
        consumers: &HashMap<OID, Vec<QueueConsumerSnapshot>>,
        activations: &HashMap<OID, ServerQueueActivation>,
    ) -> RS<ServerQueuesResponse> {
        let oid_by_name: HashMap<(String, String), OID> = schemas
            .iter()
            .map(|schema| {
                (
                    (schema.schema().clone(), schema.class_name().clone()),
                    schema.id(),
                )
            })
            .collect();
        // A queue is listed as another queue's DLQ (not as its own entry)
        // when a queue named `<name>_dlq` would collide with it in the same
        // schema.
        let dlq_names: HashSet<(String, String)> = schemas
            .iter()
            .map(|schema| (schema.schema().clone(), queue_dlq_name(schema.class_name())))
            .collect();
        let mut queues = Vec::new();
        for schema in schemas {
            let key = (schema.schema().clone(), schema.class_name().clone());
            if dlq_names.contains(&key) {
                continue;
            }
            queues.push(
                self.assemble_one_queue(schema, stats, consumers, activations, &oid_by_name)
                    .await?,
            );
        }
        queues.sort_by(|a, b| a.schema.cmp(&b.schema).then_with(|| a.name.cmp(&b.name)));
        Ok(ServerQueuesResponse {
            mode: "ok".to_string(),
            queues,
        })
    }

    /// Assembles the response entry of one queue from its shard stats,
    /// cursor list, and activation registration.
    async fn assemble_one_queue(
        &self,
        schema: &SchemaClass,
        stats: &HashMap<OID, Vec<(u32, ::xcontract::queue_types::QueueRuntimeSnapshot)>>,
        consumers: &HashMap<OID, Vec<QueueConsumerSnapshot>>,
        activations: &HashMap<OID, ServerQueueActivation>,
        oid_by_name: &HashMap<(String, String), OID>,
    ) -> RS<ServerQueueStatus> {
        let Some(shard_stats) = stats.get(&schema.id()) else {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                format!(
                    "queue {}.{} has no runtime on any live worker",
                    schema.schema(),
                    schema.class_name()
                )
            ));
        };
        let shard_index_of = self.shard_index_lookup(schema.id()).await?;
        let mut shards: Vec<ServerQueueShardStatus> = shard_stats
            .iter()
            .map(|(worker_index, snapshot)| ServerQueueShardStatus {
                shard: shard_index_of
                    .get(&snapshot.partition_id())
                    .copied()
                    .unwrap_or(0),
                worker_index: *worker_index,
                depth: snapshot.depth(),
                bytes: snapshot.bytes(),
                next_seq: snapshot.next_seq(),
                acked_frontier: snapshot.acked_frontier(),
                waiters: snapshot.waiters(),
            })
            .collect();
        shards.sort_by_key(|shard| shard.shard);
        let dlq_depth = oid_by_name
            .get(&(schema.schema().clone(), queue_dlq_name(schema.class_name())))
            .map(|dlq_oid| {
                stats
                    .get(dlq_oid)
                    .map(|entries| entries.iter().map(|(_, snapshot)| snapshot.depth()).sum())
                    .unwrap_or(0)
            });
        // The cursor lag is the cursor's backlog on its shard: messages
        // allocated at or past its frontier (`next_seq - frontier`).
        let consumers = consumers
            .get(&schema.id())
            .map(|list| {
                list.iter()
                    .map(|cursor| {
                        let shard_next = shards
                            .iter()
                            .find(|shard| shard.shard == cursor.shard)
                            .map(|shard| shard.next_seq)
                            .unwrap_or(0);
                        ServerQueueConsumer {
                            name: cursor.name.clone(),
                            shard: cursor.shard,
                            frontier: cursor.frontier,
                            lag: shard_next.saturating_sub(cursor.frontier),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(ServerQueueStatus {
            schema: schema.schema().clone(),
            name: schema.class_name().clone(),
            queue_id: schema.id(),
            worker_index: shard_stats.first().map(|(index, _)| *index).unwrap_or(0),
            depth: shards.iter().map(|shard| shard.depth).sum(),
            bytes: shards.iter().map(|shard| shard.bytes).sum(),
            overflow_count: shard_stats
                .iter()
                .map(|(_, snapshot)| snapshot.overflow_count())
                .sum(),
            next_seq: shards.iter().map(|shard| shard.next_seq).sum(),
            acked_frontier: shards.iter().map(|shard| shard.acked_frontier).sum(),
            dlq_depth,
            waiters: shards.iter().map(|shard| shard.waiters).sum(),
            shards,
            consumers,
            activation: activations.get(&schema.id()).cloned(),
        })
    }

    /// Maps the queue's shard partition OIDs to their shard indexes
    /// (positions in the bound rule's partition list); empty for an
    /// unpartitioned queue.
    async fn shard_index_lookup(&self, queue_oid: OID) -> RS<HashMap<OID, u64>> {
        let Some(binding) = self.meta_mgr.get_table_partition_binding(queue_oid).await? else {
            return Ok(HashMap::new());
        };
        let rule = self
            .meta_mgr
            .get_partition_rule_by_id(binding.rule_id)
            .await?;
        Ok(rule
            .partitions
            .iter()
            .enumerate()
            .map(|(index, partition)| (partition.partition_id, index as u64))
            .collect())
    }
}

/// A management reply of an unexpected variant is an internal protocol
/// error.
fn unexpected_reply(what: &str, reply: ManagementResponse) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::Internal,
        format!("unexpected management reply for {what}: {reply:?}")
    )
}
