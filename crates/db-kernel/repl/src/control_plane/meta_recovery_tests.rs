#![allow(clippy::unwrap_used)]
use super::{
    machine::{MetaApplier, MetaStreamMachine},
    streams::meta_stream_descriptor,
};
use crate::log_stream::{frontier::encode, DurableStreamLog, FrontierStore, StreamStateMachine};
use catalog::contract::meta_op::MetaOp;
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use mudu_raft::stream::{GroupId, GroupIdobject, StreamProgress};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

#[derive(Default)]
struct Applier {
    validations: AtomicU64,
    reject: AtomicBool,
}
#[async_trait::async_trait]
impl MetaApplier for Applier {
    async fn validate_recovery(
        &self,
        _log: &DurableStreamLog,
        _applied: Option<&StreamProgress>,
        _committed: Option<&StreamProgress>,
    ) -> RS<()> {
        self.validations.fetch_add(1, Ordering::SeqCst);
        if self.reject.load(Ordering::SeqCst) {
            return Err(mudu_error!(
                ErrorCode::CorruptedData,
                "business recovery rejected"
            ));
        }
        Ok(())
    }
    async fn apply(&self, _op: MetaOp, _version: u64) -> RS<()> {
        Ok(())
    }
}

#[test]
fn recovery_checks_authoritative_cut_before_business_hook_and_request_publication() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("meta-proof-{}", mudu_sys::random::uuid_v4()));
        let descriptor = meta_stream_descriptor();
        let log = Arc::new(
            DurableStreamLog::open(
                dir.join("log"),
                GroupIdobject {
                    cluster_id: 7,
                    group_id: GroupId(1),
                    generation: 1,
                },
                descriptor.clone(),
            )
            .await
            .unwrap(),
        );
        let applier = Arc::new(Applier::default());
        let machine = MetaStreamMachine::open(&log, applier.clone(), &dir.join("apply"), None)
            .await
            .unwrap();
        assert_eq!(applier.validations.load(Ordering::SeqCst), 1);
        let (prefix, _) = log
            .append(19, encode(&MetaOp::DropTable { oid: 7 }).unwrap(), vec![])
            .await
            .unwrap();
        machine
            .apply_once(19, log.record(0).await.unwrap().1)
            .await
            .unwrap();
        drop(machine);
        assert!(
            MetaStreamMachine::open(&log, applier.clone(), &dir.join("apply"), None)
                .await
                .is_err()
        );
        let mut wrong = prefix.clone();
        wrong.segment_digest[0] ^= 1;
        assert!(
            MetaStreamMachine::open(&log, applier.clone(), &dir.join("apply"), Some(&wrong))
                .await
                .is_err()
        );
        assert_eq!(applier.validations.load(Ordering::SeqCst), 1);
        applier.reject.store(true, Ordering::SeqCst);
        assert!(
            MetaStreamMachine::open(&log, applier.clone(), &dir.join("apply"), Some(&prefix))
                .await
                .is_err()
        );
        assert_eq!(applier.validations.load(Ordering::SeqCst), 2);
        applier.reject.store(false, Ordering::SeqCst);
        let recovered =
            MetaStreamMachine::open(&log, applier.clone(), &dir.join("apply"), Some(&prefix))
                .await
                .unwrap();
        assert!(recovered.request_applied(19).unwrap());
        let (frontier, _) =
            FrontierStore::open(&dir.join("apply"), "meta.frontier", descriptor.stream_id, 1)
                .await
                .unwrap();
        frontier
            .store(&prefix, b"unsupported checkpoint")
            .await
            .unwrap();
        assert!(
            MetaStreamMachine::open(&log, applier, &dir.join("apply"), Some(&prefix))
                .await
                .is_err()
        );
    })
    .unwrap();
}
