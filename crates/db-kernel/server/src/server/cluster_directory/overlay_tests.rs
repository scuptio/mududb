#![allow(clippy::unwrap_used)]
use super::*;
use std::path::{Path, PathBuf};
fn root() -> PathBuf {
    mudu_sys::env_var::temp_dir().join(format!("overlay-{}", mudu_sys::random::uuid_v4()))
}
fn open(path: &Path) -> Arc<ClusterDirectory> {
    ClusterDirectory::new_with_overlay(tests::topology(), 1, Some(path.to_path_buf())).unwrap()
}
fn begin(partition: &str, group: u64) -> MetaOp {
    MetaOp::BeginMigration {
        rule: "r".into(),
        partition: partition.into(),
        partition_id: 17,
        target_group: group,
        target_worker_index: 0,
        fence_reads: true,
    }
}
fn abort(partition: &str) -> MetaOp {
    MetaOp::AbortMigration {
        rule: "r".into(),
        partition: partition.into(),
    }
}

#[test]
fn abort_retains_version_and_prevents_stale_fence_after_restart() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let root = root();
        let directory = open(&root);
        directory
            .apply_placement_op(&begin("p", 2), 0)
            .await
            .unwrap();
        directory.apply_placement_op(&abort("p"), 1).await.unwrap();
        assert!(!directory.any_migrating());
        let reopened = open(&root);
        assert_eq!(reopened.placement_version(), 1);
        reopened
            .apply_placement_op(&begin("p", 2), 0)
            .await
            .unwrap();
        reopened.apply_placement_op(&abort("p"), 1).await.unwrap();
        assert!(!reopened.any_migrating());
        assert!(reopened
            .apply_placement_op(&begin("p", 2), 1)
            .await
            .is_err());
        assert!(!open(&root).any_migrating());
    })
    .unwrap();
}

#[test]
fn concurrent_placement_updates_preserve_the_whole_image() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let root = root();
        let directory = open(&root);
        let first = begin("p", 2);
        let second = begin("q", 2);
        let (a, b) = tokio::join!(biased; directory.apply_placement_op(&first, 1), directory.apply_placement_op(&second, 2));
        a.unwrap(); b.unwrap();
        let reopened = open(&root);
        for partition in ["p", "q"] {
            assert!(matches!(reopened.migration_state("r", partition), MigrationState::Migrating { fence_reads: true, .. }));
        }
        let complete = MetaOp::CompleteMigration { rule: "r".into(), partition: "p".into() };
        reopened.apply_placement_op(&complete, 3).await.unwrap();
        reopened.apply_placement_op(&complete, 3).await.unwrap();
        let final_state = open(&root);
        assert_eq!(final_state.placement("r", "p"), Some((2, 0)));
        assert_eq!(final_state.placement_version(), 3);
    }).unwrap();
}

#[test]
fn malformed_foreign_and_legacy_images_never_fall_back_to_empty() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let root = root();
        open(&root)
            .apply_placement_op(&begin("p", 2), 1)
            .await
            .unwrap();
        let fs = mudu_sys::fs::tokio_fs();
        let path = root.join("placement_overlay.mp");
        let original = fs.read_all(&path).await.unwrap();
        let mut foreign = tests::topology();
        foreign.cluster_id = 999;
        assert!(ClusterDirectory::new_with_overlay(foreign, 1, Some(root.clone())).is_err());
        repl::durable_io::publish(fs.as_ref(), &path, b"corrupt")
            .await
            .unwrap();
        assert!(
            ClusterDirectory::new_with_overlay(tests::topology(), 1, Some(root.clone())).is_err()
        );
        repl::durable_io::publish(fs.as_ref(), &path, &original)
            .await
            .unwrap();
        // Unpublished temporary files do not supersede the sealed image.
        repl::durable_io::publish(fs.as_ref(), &root.join(".publish-interrupted.tmp"), b"torn")
            .await
            .unwrap();
        assert!(open(&root).any_migrating());
        repl::durable_io::publish(fs.as_ref(), &root.join("placement_overlay_a.mp"), b"legacy")
            .await
            .unwrap();
        assert!(ClusterDirectory::new_with_overlay(tests::topology(), 1, Some(root)).is_err());
    })
    .unwrap();
}

#[test]
fn failed_publication_fences_routing_and_requires_reopen() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let root = root();
        let directory = open(&root);
        directory
            .apply_placement_op(&begin("p", 2), 1)
            .await
            .unwrap();
        let fs = mudu_sys::fs::tokio_fs();
        let path = root.join("placement_overlay.mp");
        let bytes = fs.read_all(&path).await.unwrap();
        fs.remove_file_if_exists(&path).await.unwrap();
        fs.create_dir_all(&path).await.unwrap();
        assert!(directory.apply_placement_op(&abort("p"), 2).await.is_err());
        assert_eq!(directory.placement_version(), 1);
        assert!(directory.any_migrating());
        assert!(directory.resolve_group_worker(1, 0).is_err());
        assert!(matches!(
            directory.migration_state("r", "unrelated"),
            MigrationState::Migrating {
                fence_reads: true,
                ..
            }
        ));
        fs.remove_dir_all(&path).await.unwrap();
        repl::durable_io::publish(fs.as_ref(), &path, &bytes)
            .await
            .unwrap();
        assert!(directory.apply_placement_op(&abort("p"), 2).await.is_err());
        let reopened = open(&root);
        reopened.apply_placement_op(&abort("p"), 2).await.unwrap();
        assert!(!open(&root).any_migrating());
    })
    .unwrap();
}
