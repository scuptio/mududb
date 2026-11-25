//! Client side of the migration partition RPCs: the adopting worker pulls the
//! partition's snapshot from the source owner (`PartitionSnapshotOffer` +
//! `PartitionSnapshotFetch`). The move driver speaks to the workers
//! directly (it runs on the management plane, outside a worker context).

use super::*;

impl WorkerXContract {
    /// `PartitionSnapshotOffer` against `target_worker_id`: the partition's
    /// relation-file manifest `(path, len, crc64)`.
    pub(crate) async fn remote_snapshot_offer(
        &self,
        target_worker_id: OID,
        partition_id: OID,
    ) -> RS<Vec<(String, u64, u64)>> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::PartitionSnapshotOffer { partition_id },
            )
            .await?
        {
            PartitionRpcResponse::PartitionSnapshotOffer(files) => Ok(files),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected snapshot offer rpc response"
            )),
        }
    }

    /// `PartitionSnapshotFetch`: one page of an offered file.
    pub(crate) async fn remote_snapshot_fetch(
        &self,
        target_worker_id: OID,
        partition_id: OID,
        path: &str,
        offset: u64,
        len: u64,
    ) -> RS<Vec<u8>> {
        match self
            .send_partition_rpc(
                target_worker_id,
                PartitionRpcRequest::PartitionSnapshotFetch {
                    partition_id,
                    path: path.to_string(),
                    offset,
                    len,
                },
            )
            .await?
        {
            PartitionRpcResponse::PartitionSnapshotFetch(data) => Ok(data),
            PartitionRpcResponse::Err(err) => Err(mudu_error!(ErrorCode::Internal, err)),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "unexpected snapshot fetch rpc response"
            )),
        }
    }
}
