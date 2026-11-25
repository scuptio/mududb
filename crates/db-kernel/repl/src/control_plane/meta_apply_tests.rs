#![allow(clippy::unwrap_used)]
use super::{
    machine::{MetaApplier, MetaStreamMachine},
    streams::meta_stream_descriptor,
};
use crate::log_stream::{frontier::encode, DurableStreamLog, StreamStateMachine, StreamStruct};
use catalog::contract::meta_op::MetaOp;
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use mudu_raft::stream::{GroupId, GroupIdobject, StreamProgress};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicU8, Ordering},
        Arc,
    },
};

#[derive(Default)]
struct Applier {
    calls: AtomicU64,
    mode: AtomicU8,
    entered: mudu_sys::tokio::sync::Notify,
    release: mudu_sys::tokio::sync::Notify,
}
#[async_trait::async_trait]
impl MetaApplier for Applier {
    async fn validate_recovery(
        &self,
        _: &DurableStreamLog,
        _: Option<&StreamProgress>,
        _: Option<&StreamProgress>,
    ) -> RS<()> {
        Ok(())
    }
    async fn apply(&self, _: MetaOp, _: u64) -> RS<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.mode.load(Ordering::SeqCst) {
            1 => return Err(mudu_error!(ErrorCode::Internal, "injected apply failure")),
            2 => {
                self.entered.notify_one();
                self.release.notified().await;
            }
            _ => (),
        }
        Ok(())
    }
}

struct Fixture {
    dir: PathBuf,
    log: Arc<DurableStreamLog>,
    applier: Arc<Applier>,
    machine: MetaStreamMachine,
}
impl Fixture {
    async fn new() -> Self {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("meta-order-{}", mudu_sys::random::uuid_v4()));
        let log = Arc::new(
            DurableStreamLog::open(
                dir.join("log"),
                GroupIdobject {
                    cluster_id: 7,
                    group_id: GroupId(1),
                    generation: 1,
                },
                meta_stream_descriptor(),
            )
            .await
            .unwrap(),
        );
        for id in 10..13 {
            log.append(id, encode(&MetaOp::DropTable { oid: id }).unwrap(), vec![])
                .await
                .unwrap();
        }
        let applier = Arc::new(Applier::default());
        let cut = log.record(2).await.unwrap().1.progress;
        let machine =
            MetaStreamMachine::open(&log, applier.clone(), &dir.join("apply"), Some(&cut))
                .await
                .unwrap();
        Self {
            dir,
            log,
            applier,
            machine,
        }
    }
    async fn record(&self, lsn: u64) -> StreamStruct {
        self.log.record(lsn).await.unwrap().1
    }
    async fn reopen(&self) -> MetaStreamMachine {
        let cut = self.record(2).await.progress;
        MetaStreamMachine::open(
            &self.log,
            self.applier.clone(),
            &self.dir.join("apply"),
            Some(&cut),
        )
        .await
        .unwrap()
    }
}

#[test]
fn metadata_rejects_wrong_records_before_business_mutation() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        let first = f.record(0).await;
        for case in 0..8 {
            let mut record = first.clone();
            let mut request = 10;
            match case {
                0 => record.identity.cluster_id += 1,
                1 => record.identity.generation += 1,
                2 => record.progress.stream_id.0 += 1,
                3 => record.progress.stream_generation += 1,
                4 => record.progress.segment_digest[0] ^= 1,
                5 => record.payload = encode(&MetaOp::DropTable { oid: 99 }).unwrap(),
                6 => record.dependencies.push(first.progress.clone()),
                _ => request += 1,
            }
            assert!(f.machine.apply_once(request, record).await.is_err());
        }
        assert!(f.machine.apply_once(11, f.record(1).await).await.is_err());
        assert_eq!(f.applier.calls.load(Ordering::SeqCst), 0);
        assert!(f.machine.applied_frontier().await.unwrap().is_none());
        assert!(!f.machine.request_applied(10).unwrap());
        f.machine.apply_once(10, first).await.unwrap();
        assert_eq!(f.applier.calls.load(Ordering::SeqCst), 1);
    })
    .unwrap();
}

#[test]
fn concurrent_metadata_retries_apply_once_and_never_regress_after_reopen() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        let first = f.record(0).await;
        let (a, b) = tokio::join!(
            f.machine.apply_once(10, first.clone()),
            f.machine.apply_once(10, first.clone())
        );
        a.unwrap();
        b.unwrap();
        assert_eq!(f.applier.calls.load(Ordering::SeqCst), 1);
        let second = f.record(1).await;
        f.machine.apply_once(11, second.clone()).await.unwrap();
        f.machine.apply_once(10, first.clone()).await.unwrap();
        assert_eq!(
            f.machine.applied_frontier().await.unwrap(),
            Some(second.progress.clone())
        );
        let reopened = f.reopen().await;
        reopened.apply_once(10, first).await.unwrap();
        reopened.apply_once(11, second.clone()).await.unwrap();
        assert_eq!(f.applier.calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            reopened.applied_frontier().await.unwrap(),
            Some(second.progress)
        );
        assert!(reopened.request_applied(10).unwrap());
        assert!(reopened.request_applied(11).unwrap());
    })
    .unwrap();
}

#[test]
fn failed_or_cancelled_metadata_apply_requires_recovery_before_more_effects() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for mode in [1,2] {
            let f = Fixture::new().await;
            let first = f.record(0).await;
            f.applier.mode.store(mode, Ordering::SeqCst);
            if mode == 1 {
                assert!(f.machine.apply_once(10, first.clone()).await.is_err());
            } else {
                tokio::select! {
                    _ = f.machine.apply_once(10, first.clone()) => panic!("apply unexpectedly completed"),
                    _ = f.applier.entered.notified() => (),
                }
            }
            f.applier.mode.store(0, Ordering::SeqCst);
            assert!(f.machine.apply_once(10, first.clone()).await.is_err());
            assert!(f.machine.apply_once(11, f.record(1).await).await.is_err());
            assert_eq!(f.applier.calls.load(Ordering::SeqCst), 1);
            assert!(!f.machine.request_applied(10).unwrap());
            assert!(f.machine.applied_frontier().await.unwrap().is_none());
            let reopened = f.reopen().await;
            reopened.apply_once(10, first).await.unwrap();
            assert!(reopened.request_applied(10).unwrap());
            assert_eq!(f.applier.calls.load(Ordering::SeqCst), 2);
        }
    }).unwrap();
}

#[test]
fn metadata_frontier_publish_failure_does_not_publish_request_success() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        let path = f.dir.join("apply/meta.frontier");
        let fs = mudu_sys::fs::tokio_fs();
        // Force rename publication to fail after the business hook succeeds.
        fs.create_dir_all(&path).await.unwrap();
        let record = f.record(0).await;
        assert!(f.machine.apply_once(10, record.clone()).await.is_err());
        assert_eq!(f.applier.calls.load(Ordering::SeqCst), 1);
        assert!(!f.machine.request_applied(10).unwrap());
        assert!(f.machine.applied_frontier().await.unwrap().is_none());
        fs.remove_dir_all(&path).await.unwrap();
        assert!(f.machine.apply_once(10, record.clone()).await.is_err());
        assert_eq!(f.applier.calls.load(Ordering::SeqCst), 1);
        let reopened = f.reopen().await;
        reopened.apply_once(10, record).await.unwrap();
        assert!(reopened.request_applied(10).unwrap());
    })
    .unwrap();
}

#[test]
fn metadata_retry_rejects_replaced_applied_history() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        f.machine.apply_once(10, f.record(0).await).await.unwrap();
        // An invalid heal must not turn a different payload under the same
        // request/LSN into a successful retry of the prior catalog mutation.
        f.log.truncate_uncommitted(None).await.unwrap();
        f.log
            .append(10, encode(&MetaOp::DropTable { oid: 999 }).unwrap(), vec![])
            .await
            .unwrap();
        assert!(f.machine.apply_once(10, f.record(0).await).await.is_err());
        assert_eq!(f.applier.calls.load(Ordering::SeqCst), 1);
    })
    .unwrap();
}

mod result_tests;
