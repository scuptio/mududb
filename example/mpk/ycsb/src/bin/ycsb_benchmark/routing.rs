use mudu_client::management::{ServerTopology, fetch_server_topology};
use mududb::common::result::RS;
use tokio::runtime::Builder;

#[derive(Clone)]
pub(crate) struct PartitionRouting {
    slots: Vec<PartitionSlot>,
}

#[derive(Clone, Copy)]
pub(crate) struct PartitionSlot {
    pub(crate) partition_id: u128,
    pub(crate) worker_id: u128,
}

impl PartitionRouting {
    pub(crate) fn slot(&self, partition_index: usize) -> RS<PartitionSlot> {
        self.slots.get(partition_index).copied().ok_or_else(|| {
            mududb::mudu_error!(
                mududb::error::ErrorCode::EntityNotFound,
                format!("no routing slot for partition index {}", partition_index)
            )
        })
    }
}

pub(crate) fn load_partition_routing_sync(partition_count: usize) -> RS<PartitionRouting> {
    let Some(http_addr) = mudu_adapter::config::mudud_http_addr() else {
        return Ok(default_partition_routing(partition_count));
    };
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            mududb::mudu_error!(
                mududb::error::ErrorCode::Tokio,
                "build ycsb topology runtime error",
                e
            )
        })?;
    let topology = match runtime.block_on(fetch_server_topology(&http_addr)) {
        Ok(topology) => topology,
        Err(err) if topology_is_unsupported(&err) => {
            return Ok(default_partition_routing(partition_count));
        }
        Err(err) => return Err(mududb::mudu_error!(mududb::error::ErrorCode::Network, err)),
    };
    build_partition_routing(partition_count, topology)
}

pub(crate) async fn load_partition_routing_async(partition_count: usize) -> RS<PartitionRouting> {
    let Some(http_addr) = mudu_adapter::config::mudud_http_addr() else {
        return Ok(default_partition_routing(partition_count));
    };
    let topology = match fetch_server_topology(&http_addr).await {
        Ok(topology) => topology,
        Err(err) if topology_is_unsupported(&err) => {
            return Ok(default_partition_routing(partition_count));
        }
        Err(err) => return Err(mududb::mudu_error!(mududb::error::ErrorCode::Network, err)),
    };
    build_partition_routing(partition_count, topology)
}

fn topology_is_unsupported(err: &str) -> bool {
    err.contains("server topology is not supported") || err.contains("\"code\":\"NotImplemented\"")
}

fn default_partition_routing(partition_count: usize) -> PartitionRouting {
    PartitionRouting {
        slots: (0..partition_count)
            .map(|partition_index| PartitionSlot {
                partition_id: partition_index as u128 + 1,
                worker_id: 0,
            })
            .collect(),
    }
}

fn build_partition_routing(
    partition_count: usize,
    topology: ServerTopology,
) -> RS<PartitionRouting> {
    let mut slots = Vec::new();
    for worker in topology.workers {
        for partition_id in worker.partitions {
            slots.push(PartitionSlot {
                partition_id,
                worker_id: worker.worker_id,
            });
        }
    }
    slots.sort_by_key(|slot| slot.partition_id);
    if slots.len() < partition_count {
        return Err(mududb::mudu_error!(
            mududb::error::ErrorCode::EntityNotFound,
            format!(
                "requested {} partitions but topology only exposes {}",
                partition_count,
                slots.len()
            )
        ));
    }
    Ok(PartitionRouting {
        slots: slots.into_iter().take(partition_count).collect(),
    })
}
