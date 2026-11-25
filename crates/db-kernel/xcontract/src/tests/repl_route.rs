//! Replication commit routing: replica rejection, primary async, primary
//! sync (ack wait) and the degraded sync-without-waiter path.

use super::*;
use crate::contract::WorkerXContract;
use crate::types::ReplCommitRoute;
use ::catalog::repl_ctrl::{ReplAckWait, ReplControl};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_sys::env_var::temp_dir;
use mudu_sys::sync::SMutex;
use mudu_utils::oid::gen_oid;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

struct MockReplCtl {
    primary: bool,
    sync: bool,
    epoch: Arc<AtomicU64>,
    apply_begins: AtomicUsize,
    apply_ends: AtomicUsize,
}

impl ReplControl for MockReplCtl {
    fn is_primary(&self) -> bool {
        self.primary
    }
    fn sync(&self) -> bool {
        self.sync
    }
    fn sync_timeout(&self) -> Duration {
        Duration::from_millis(50)
    }
    fn leader_hint(&self) -> Option<mudu_contract::protocol::LeaderHint> {
        None
    }
    fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Relaxed)
    }
    fn epoch_source(&self) -> Arc<AtomicU64> {
        self.epoch.clone()
    }
    fn note_epoch(&self, epoch: u64) {
        self.epoch.fetch_max(epoch, Ordering::Relaxed);
    }
    fn set_primary(&self, _new_epoch: u64) -> RS<()> {
        Ok(())
    }
    fn set_follow(
        &self,
        _primary_node_id: u64,
        _primary_addr: String,
        _leader_hint: Option<mudu_contract::protocol::LeaderHint>,
    ) -> RS<()> {
        Ok(())
    }
    fn primary_node_id(&self) -> Option<u64> {
        None
    }
    fn primary_addr(&self) -> Option<String> {
        None
    }
    fn applied_low_water(&self) -> u64 {
        0
    }
    fn apply_track_begin(&self) {
        self.apply_begins.fetch_add(1, Ordering::Relaxed);
    }
    fn apply_track_end(&self, _allocated_lsn: u64) {
        self.apply_ends.fetch_add(1, Ordering::Relaxed);
    }
}

struct MockAckWait {
    covered: SMutex<Vec<(u64, Duration)>>,
}

#[async_trait::async_trait]
impl ReplAckWait for MockAckWait {
    async fn wait_cover(&self, lsn: u64, timeout: Duration) -> RS<()> {
        self.covered.lock()?.push((lsn, timeout));
        Ok(())
    }
}

async fn contract_with_ctl(
    ctl: Option<Arc<MockReplCtl>>,
    waiter: Option<Arc<MockAckWait>>,
) -> WorkerXContract {
    // The replication route block runs only on the logged commit path, so
    // the fixture carries a real (small-chunk) worker log.
    let data_dir = temp_dir()
        .join(format!("repl_route_test_{}", gen_oid()))
        .to_string_lossy()
        .to_string();
    let log_dir = temp_dir()
        .join(format!("repl_route_log_{}", gen_oid()))
        .to_string_lossy()
        .to_string();
    let log = crate::ChunkedWorkerLogBackend::new(
        wal::worker_log::WorkerLogLayout::new(log_dir, gen_oid(), 4096).unwrap(),
    )
    .await
    .unwrap();
    let contract = WorkerXContract::with_worker_log_and_data_dir(log, 0, 0, 0, data_dir)
        .await
        .unwrap();
    if let Some(ctl) = ctl {
        contract.set_repl_ctl(ctl).unwrap();
    }
    if let Some(waiter) = waiter {
        contract.set_repl_ack_waiter(waiter).unwrap();
    }
    contract
}

#[test]
fn replica_rejects_commits() {
    block_on(async {
        let ctl = Arc::new(MockReplCtl {
            primary: false,
            sync: false,
            epoch: Arc::new(AtomicU64::new(1)),
            apply_begins: AtomicUsize::new(0),
            apply_ends: AtomicUsize::new(0),
        });
        let contract = contract_with_ctl(Some(ctl.clone()), None).await;
        // A KV write on a StreamFollower is rejected with a leader-hint error, and
        // nothing is applied or tracked.
        let err = contract
            .worker_put_async(b"k".to_vec(), b"v".to_vec())
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::DomainViolation);
        // The delete path shares the same routing block.
        let err = contract.worker_delete_async(b"k").await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::DomainViolation);
        // The apply span opens before the route check and closes when it
        // settles, so each rejected commit balances begin/end.
        assert_eq!(ctl.apply_begins.load(Ordering::Relaxed), 2);
        assert_eq!(ctl.apply_ends.load(Ordering::Relaxed), 2);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap()
}

#[test]
fn primary_async_commits_inline_and_tracks_apply() {
    block_on(async {
        let ctl = Arc::new(MockReplCtl {
            primary: true,
            sync: false,
            epoch: Arc::new(AtomicU64::new(1)),
            apply_begins: AtomicUsize::new(0),
            apply_ends: AtomicUsize::new(0),
        });
        let contract = contract_with_ctl(Some(ctl.clone()), None).await;
        contract
            .worker_put_async(b"k".to_vec(), b"v".to_vec())
            .await?;
        assert_eq!(contract.worker_get_async(b"k").await?, Some(b"v".to_vec()));
        // The delete commit takes the same routing block.
        contract.worker_delete_async(b"k").await?;
        assert_eq!(contract.worker_get_async(b"k").await?, None);
        // The apply span wraps the write for the low-watermark reporter.
        assert_eq!(ctl.apply_begins.load(Ordering::Relaxed), 2);
        assert_eq!(ctl.apply_ends.load(Ordering::Relaxed), 2);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap()
}

#[test]
fn primary_sync_waits_for_replica_ack() {
    block_on(async {
        let ctl = Arc::new(MockReplCtl {
            primary: true,
            sync: true,
            epoch: Arc::new(AtomicU64::new(1)),
            apply_begins: AtomicUsize::new(0),
            apply_ends: AtomicUsize::new(0),
        });
        let waiter = Arc::new(MockAckWait {
            covered: SMutex::new(vec![]),
        });
        let contract = contract_with_ctl(Some(ctl), Some(waiter.clone())).await;
        contract
            .worker_put_async(b"k".to_vec(), b"v".to_vec())
            .await?;
        contract.worker_delete_async(b"k").await?;
        // Each sync commit flushed the log and waited for ack coverage of
        // the allocated LSN with the configured timeout.
        let covered = waiter.covered.lock()?;
        assert_eq!(covered.len(), 2);
        assert_eq!(covered[0].1, Duration::from_millis(50));
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap()
}

#[test]
fn primary_sync_without_waiter_degrades_to_async() {
    block_on(async {
        let ctl = Arc::new(MockReplCtl {
            primary: true,
            sync: true,
            epoch: Arc::new(AtomicU64::new(1)),
            apply_begins: AtomicUsize::new(0),
            apply_ends: AtomicUsize::new(0),
        });
        let contract = contract_with_ctl(Some(ctl), None).await;
        // No ack waiter installed: the commit degrades to async instead of
        // hanging.
        contract
            .worker_put_async(b"k".to_vec(), b"v".to_vec())
            .await?;
        assert_eq!(contract.worker_get_async(b"k").await?, Some(b"v".to_vec()));
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap()
}

#[test]
fn route_matrix_without_raft_share() {
    block_on(async {
        // No ctl at all: single-node Direct.
        let contract = contract_with_ctl(None, None).await;
        assert!(matches!(
            contract.repl_commit_route()?,
            ReplCommitRoute::Direct
        ));
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap()
}

fn batch_with_one_put() -> (
    std::collections::BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    wal::xl_batch::XLBatch,
) {
    let mut items = std::collections::BTreeMap::new();
    items.insert(b"bk".to_vec(), Some(b"bv".to_vec()));
    (items, wal::xl_batch::XLBatch::new(vec![]))
}

#[test]
fn replica_rejects_batched_commits() {
    block_on(async {
        let ctl = Arc::new(MockReplCtl {
            primary: false,
            sync: false,
            epoch: Arc::new(AtomicU64::new(1)),
            apply_begins: AtomicUsize::new(0),
            apply_ends: AtomicUsize::new(0),
        });
        let contract = contract_with_ctl(Some(ctl), None).await;
        // The batch commit requires an active transaction id.
        let tx = contract.begin_tx().await.unwrap();
        let (items, batch) = batch_with_one_put();
        let snapshot = tx.snapshot();
        let err = contract
            .worker_commit_put_batch_async(&snapshot, tx.xid(), items, batch)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::DomainViolation);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap()
}

#[test]
fn primary_sync_batched_commit_waits_for_ack() {
    block_on(async {
        let ctl = Arc::new(MockReplCtl {
            primary: true,
            sync: true,
            epoch: Arc::new(AtomicU64::new(1)),
            apply_begins: AtomicUsize::new(0),
            apply_ends: AtomicUsize::new(0),
        });
        let waiter = Arc::new(MockAckWait {
            covered: SMutex::new(vec![]),
        });
        let contract = contract_with_ctl(Some(ctl), Some(waiter.clone())).await;
        // The batch commit requires an active transaction id.
        let tx = contract.begin_tx().await.unwrap();
        let (items, batch) = batch_with_one_put();
        let snapshot = tx.snapshot();
        contract
            .worker_commit_put_batch_async(&snapshot, tx.xid(), items, batch)
            .await?;
        let covered = waiter.covered.lock()?;
        assert_eq!(covered.len(), 1);
        assert_eq!(
            contract.worker_get_async(b"bk").await?,
            Some(b"bv".to_vec())
        );
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap()
}
