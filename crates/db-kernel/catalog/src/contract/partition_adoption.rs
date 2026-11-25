//! Runtime partition adoption contract: a partition move ends with the
//! target worker adopting the partition (registry marker + live partition
//! set, so snapshots and restarts keep it) and the source worker releasing
//! it.
//!
//! Implemented in `worker` (which owns the registry and the runtime),
//! consumed by `xcontract`'s migration RPC handlers. Pure-contract crate
//! placement keeps the layering rule (xcontract must not depend on worker).

use mudu::common::id::OID;
use mudu::common::result::RS;

/// Adopts/releases partitions of a running worker.
pub trait PartitionAdoption: Send + Sync {
    /// The worker adopts `partition_id`: its registry gains the partition
    /// marker (durable across restarts) and the running runtime's partition
    /// set (so snapshots and the relation bootstrap include it).
    /// Idempotent.
    fn adopt(&self, partition_id: OID) -> RS<()>;

    /// The worker releases `partition_id` (the partition moved away):
    /// drops the registry marker and the runtime partition set entry.
    /// Idempotent.
    fn release(&self, partition_id: OID) -> RS<()>;
}
