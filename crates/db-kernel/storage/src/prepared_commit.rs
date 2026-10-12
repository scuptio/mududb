//! Prepared commit payload and the commit-sequencer interface.

use ::wal::lsn::LSN;
use ::wal::xl_batch::XLBatch;
use ::xengine::api::DeltaAssign;
use ::xengine::tx_mgr::PhysicalRelationId;
use mudu::common::result::RS;
use mudu_sys::tokio::sync::oneshot;
use std::collections::BTreeMap;

/// One fully-prepared (but not yet applied) commit: the resolved relation and
/// kv write sets, the deferred delta assignments, and the WAL batch.
pub struct PreparedWorkerCommit {
    /// Transaction id of the commit.
    pub xid: u64,
    /// Per-relation write set: row key to new value (`None` deletes the row).
    pub relation_rows: BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
    /// Per-relation deferred delta assignments, keyed by row key.
    pub relation_deltas: BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Vec<DeltaAssign>>>,
    /// Key-value write set: key to new value (`None` deletes the key).
    pub kv_rows: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    /// The WAL batch covering this commit.
    pub batch: XLBatch,
}

impl PreparedWorkerCommit {
    /// The WAL batch of this prepared commit.
    pub fn batch(&self) -> &XLBatch {
        &self.batch
    }
}

/// Staging surface for the raft leader's commit sequencer.
pub trait CommitSequencer: Send + Sync {
    /// Stage a prepared commit and return a receiver that completes once it
    /// is raft-committed and applied.
    fn stage(
        &self,
        prepared: PreparedWorkerCommit,
        frame_count: usize,
        byte_len: usize,
        last_lsn: LSN,
    ) -> RS<oneshot::Receiver<RS<()>>>;
}
