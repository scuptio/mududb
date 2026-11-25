//! The partition-move endpoint's delegate: the orchestration lives in
//! `crate::backend::migration_driver::MigrationDriver` (shared with the
//! rebalance scheduler); this module only wires the API surface to it.

use mudu::common::result::RS;

use super::super::{
    PartitionMoveAbortRequest, PartitionMoveRequest, PartitionMoveResponse, PartitionRouteEntry,
};
use super::KernelHttpApi;
use crate::backend::migration_driver::{MigrationDriver, MoveOutcome, PartitionMoveAbortRef};

impl KernelHttpApi {
    /// Moves one partition to another node group (phase 3).
    pub(super) async fn move_partition(
        &self,
        request: PartitionMoveRequest,
    ) -> RS<PartitionMoveResponse> {
        let driver = self.migration_driver()?;
        let outcome = driver.move_partition(request).await?;
        Ok(Self::move_response(outcome))
    }

    /// Aborts an in-flight partition move.
    pub(super) async fn abort_partition_move(&self, request: PartitionMoveAbortRequest) -> RS<()> {
        let driver = self.migration_driver()?;
        driver
            .abort_partition_move(PartitionMoveAbortRef {
                rule: &request.rule,
                partition: &request.partition,
            })
            .await
    }

    /// The migration driver bound to this server's cluster, or an error
    /// outside cluster mode.
    pub(super) fn migration_driver(&self) -> RS<MigrationDriver> {
        let server_instance_id = self.server_instance_id.ok_or_else(|| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::InvalidState,
                "partition moves require a running server"
            )
        })?;
        MigrationDriver::for_server(server_instance_id, self.meta_mgr.clone())?.ok_or_else(|| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::InvalidState,
                "partition moves require cluster mode"
            )
        })
    }

    /// Maps the driver's outcome to the endpoint's response.
    pub(super) fn move_response(outcome: MoveOutcome) -> PartitionMoveResponse {
        PartitionMoveResponse {
            routes: vec![PartitionRouteEntry {
                partition_id: outcome.partition_id,
                worker_id: 0,
                group_id: Some(outcome.group_id),
                node_id: outcome.node_id,
                client_addr: outcome.client_addr,
            }],
            placement_version: outcome.placement_version,
            rows_rehomed: outcome.rows_rehomed,
        }
    }
}
