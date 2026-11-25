//! The partition split/merge endpoints' delegate: the orchestration lives in
//! `crate::backend::migration_driver::MigrationDriver`; this module only
//! wires the API surface to it.

use mudu::common::result::RS;

use super::super::{PartitionMergeRequest, PartitionMoveResponse, PartitionSplitRequest};
use super::KernelHttpApi;

impl KernelHttpApi {
    /// Splits one partition at a split point.
    pub(super) async fn split_partition(
        &self,
        request: PartitionSplitRequest,
    ) -> RS<PartitionMoveResponse> {
        let outcome = self.migration_driver()?.split_partition(request).await?;
        Ok(Self::move_response(outcome))
    }

    /// Merges two adjacent partitions of one rule.
    pub(super) async fn merge_partitions(
        &self,
        request: PartitionMergeRequest,
    ) -> RS<PartitionMoveResponse> {
        let outcome = self.migration_driver()?.merge_partitions(request).await?;
        Ok(Self::move_response(outcome))
    }
}
