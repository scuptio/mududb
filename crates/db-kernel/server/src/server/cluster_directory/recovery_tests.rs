#![allow(clippy::unwrap_used)]
use super::*;
use catalog::contract::control_plane::*;
use mudu_raft::stream::{
    GroupId, GroupIdobject, StreamBudget, StreamDescriptor, StreamId, StreamProgress,
};
use repl::control_plane::MetaApplier;
use repl::log_stream::DurableStreamLog;
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    directory: Arc<ClusterDirectory>,
    log: DurableStreamLog,
}
impl Fixture {
    async fn new() -> Self {
        let root = mudu_sys::env_var::temp_dir()
            .join(format!("overlay-proof-{}", mudu_sys::random::uuid_v4()));
        let directory =
            ClusterDirectory::new_with_overlay(tests::topology(), 1, Some(root.join("data")))
                .unwrap();
        let identity = GroupIdobject {
            cluster_id: tests::topology().control_cluster_id(),
            group_id: GroupId(CONTROL_GROUP_ID),
            generation: 1,
        };
        let log = DurableStreamLog::open(
            root.join("log"),
            identity,
            StreamDescriptor {
                stream_id: StreamId(CONTROL_STREAM_META),
                stream_generation: 1,
                kind: CONTROL_STREAM_META_KIND.into(),
                codec_version: CONTROL_STREAM_CODEC_VERSION,
                digest_version: CONTROL_STREAM_DIGEST_VERSION,
                budget: StreamBudget {
                    max_record_bytes: 4 * 1024 * 1024,
                    max_inflight_bytes: 64 * 1024 * 1024,
                    priority: 2,
                },
                requires: vec![],
            },
        )
        .await
        .unwrap();
        Self {
            root,
            directory,
            log,
        }
    }
    async fn append(&self, id: u128, op: &MetaOp) -> StreamProgress {
        self.log
            .append(id, rmp_serde::to_vec(op).unwrap(), vec![])
            .await
            .unwrap()
            .0
    }
    fn reopen(&self) -> Arc<ClusterDirectory> {
        ClusterDirectory::new_with_overlay(tests::topology(), 1, Some(self.root.join("data")))
            .unwrap()
    }
}
fn begin() -> MetaOp {
    MetaOp::BeginMigration {
        rule: "r".into(),
        partition: "p".into(),
        partition_id: 9,
        target_group: 2,
        target_worker_index: 0,
        fence_reads: true,
    }
}
fn abort() -> MetaOp {
    MetaOp::AbortMigration {
        rule: "r".into(),
        partition: "p".into(),
    }
}

#[test]
fn missing_and_stale_directory_are_rejected_by_production_applier_hook() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        let first = f.append(1, &begin()).await;
        f.directory.apply_placement_op(&begin(), 0).await.unwrap();
        let fs = mudu_sys::fs::tokio_fs();
        let path = f.root.join("data/placement_overlay.mp");
        let old = fs.read_all(&path).await.unwrap();
        let second = f.append(2, &abort()).await;
        f.directory.apply_placement_op(&abort(), 1).await.unwrap();
        f.reopen()
            .validate_control_recovery(&f.log, Some(&second), Some(&second))
            .await
            .unwrap();
        fs.remove_file_if_exists(&path).await.unwrap();
        let missing = f.reopen();
        let applier = crate::server::meta_apply::NodeMetaApplier::new(
            &f.root.join("data").to_string_lossy(),
            missing.clone(),
        );
        assert!(applier
            .validate_recovery(&f.log, Some(&second), Some(&second))
            .await
            .is_err());
        assert!(!fs.path_exists(&path).await.unwrap());
        assert!(missing.resolve_group_worker(1, 0).is_err());
        repl::durable_io::publish(fs.as_ref(), &path, &old)
            .await
            .unwrap();
        assert!(f
            .reopen()
            .validate_control_recovery(&f.log, Some(&second), Some(&second))
            .await
            .is_err());
        // A valid earlier applied cut still accepts the matching directory;
        // committed suffixes have not necessarily been applied at startup.
        f.reopen()
            .validate_control_recovery(&f.log, Some(&first), Some(&second))
            .await
            .unwrap();
    })
    .unwrap();
}

#[test]
fn only_next_committed_operation_may_have_durable_effects_ahead_of_frontier() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        f.directory
            .validate_control_recovery(&f.log, None, None)
            .await
            .unwrap();
        let first = f.append(1, &begin()).await;
        f.directory.apply_placement_op(&begin(), 0).await.unwrap();
        assert!(f
            .reopen()
            .validate_control_recovery(&f.log, None, None)
            .await
            .is_err());
        f.reopen()
            .validate_control_recovery(&f.log, None, Some(&first))
            .await
            .unwrap();
        let second = f.append(2, &abort()).await;
        f.directory.apply_placement_op(&abort(), 1).await.unwrap();
        assert!(f
            .reopen()
            .validate_control_recovery(&f.log, None, Some(&second))
            .await
            .is_err());
        assert!(f
            .reopen()
            .validate_control_recovery(&f.log, Some(&first), Some(&first))
            .await
            .is_err());
        let reopened = f.reopen();
        reopened
            .validate_control_recovery(&f.log, Some(&first), Some(&second))
            .await
            .unwrap();
        reopened.apply_placement_op(&abort(), 1).await.unwrap();
        reopened
            .validate_control_recovery(&f.log, Some(&second), Some(&second))
            .await
            .unwrap();
        assert!(!reopened.any_migrating());
        let mut wrong = second.clone();
        wrong.segment_digest[0] ^= 1;
        assert!(reopened
            .validate_control_recovery(&f.log, Some(&second), Some(&wrong))
            .await
            .is_err());
    })
    .unwrap();
}

#[test]
fn cursor_and_full_directory_contents_are_proved_not_only_the_version() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        let mut wrong = begin();
        if let MetaOp::BeginMigration { target_group, .. } = &mut wrong {
            *target_group = 1;
        }
        let first = f.append(1, &begin()).await;
        f.directory.apply_placement_op(&wrong, 0).await.unwrap();
        assert!(f
            .reopen()
            .validate_control_recovery(&f.log, Some(&first), Some(&first))
            .await
            .is_err());
        // A later matching cursor cannot hide a wrong earlier entry.
        let second_op = MetaOp::CompleteMigration {
            rule: "r".into(),
            partition: "p".into(),
        };
        let second = f.append(2, &second_op).await;
        f.directory.apply_placement_op(&second_op, 1).await.unwrap();
        assert!(f
            .reopen()
            .validate_control_recovery(&f.log, Some(&second), Some(&second))
            .await
            .is_err());
    })
    .unwrap();
}

#[test]
fn nonplacement_history_does_not_require_an_overlay_file() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        let prefix = f.append(1, &MetaOp::DropTable { oid: 99 }).await;
        f.directory
            .validate_control_recovery(&f.log, Some(&prefix), Some(&prefix))
            .await
            .unwrap();
        assert!(!mudu_sys::fs::tokio_fs()
            .path_exists(&f.root.join("data/placement_overlay.mp"))
            .await
            .unwrap());
    })
    .unwrap();
}
