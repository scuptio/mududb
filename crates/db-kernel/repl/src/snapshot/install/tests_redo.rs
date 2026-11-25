#![allow(clippy::unwrap_used)]
use super::*;
use worker::worker_registry::WorkerIdobject;

fn identity() -> WorkerIdobject {
    WorkerIdobject {
        worker_index: 0,
        worker_id: 42,
        partition_ids: vec![0],
    }
}

#[test]
fn redo_after_each_file_phase_and_base_publication_is_idempotent() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for stage in 0..3 {
            let (root, manifest) = tests::source().await;
            let relation = Path::new(&root).join("relation");
            mudu_sys::fs::sync::create_dir_all(&relation).unwrap();
            mudu_sys::fs::sync::write(relation.join("0.1.dat"), b"stale").unwrap();
            let journal = InstallJournal::begin(&root, 42, &[0], &manifest)
                .await
                .unwrap();
            if stage >= 1 {
                journal.install_files().await.unwrap();
            }
            if stage >= 2 {
                super::super::base::publish(&root, 42, &manifest)
                    .await
                    .unwrap();
            }
            drop(journal);
            for _ in 0..2 {
                recover_pending_installs(&root, &format!("{root}/log"), 4096, &[identity()])
                    .await
                    .unwrap();
                ensure_no_pending_installs(&root).unwrap();
                assert!(!mudu_sys::fs::sync::sync_path_exists(
                    relation.join("0.1.dat")
                ));
                assert_eq!(
                    super::super::base::read(&snapshot_worker_dir(&root, 0))
                        .unwrap()
                        .unwrap()
                        .manifest,
                    manifest
                );
            }
        }
    })
    .unwrap();
}

#[test]
fn redo_rejects_changed_identity_ownership_and_corrupt_intent() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (root, manifest) = tests::source().await;
        let journal = InstallJournal::begin(&root, 42, &[0], &manifest)
            .await
            .unwrap();
        for target in [
            WorkerIdobject {
                worker_id: 43,
                ..identity()
            },
            WorkerIdobject {
                partition_ids: vec![7],
                ..identity()
            },
        ] {
            assert!(recover_pending_installs(&root, &root, 4096, &[target])
                .await
                .is_err());
        }
        assert!(recover_pending_installs(&root, &root, 4096, &[])
            .await
            .is_err());
        let mut envelope: CheckedIntent = rmp_serde::from_slice(&journal.bytes).unwrap();
        envelope.bytes[0] ^= 1;
        mudu_sys::fs::sync::write(&journal.path, rmp_serde::to_vec(&envelope).unwrap()).unwrap();
        assert!(recover_pending_installs(&root, &root, 4096, &[identity()])
            .await
            .is_err());
        assert!(super::super::base::read(&snapshot_worker_dir(&root, 0))
            .unwrap()
            .is_none());
        assert!(ensure_no_pending_installs(&root).is_err());
    })
    .unwrap();
}

#[test]
fn all_pending_records_are_validated_before_any_live_deletion() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (root, manifest) = tests::source().await;
        let relation = Path::new(&root).join("relation");
        mudu_sys::fs::sync::create_dir_all(&relation).unwrap();
        let stale = relation.join("0.1.dat");
        mudu_sys::fs::sync::write(&stale, b"retain").unwrap();
        let _journal = InstallJournal::begin(&root, 42, &[0], &manifest)
            .await
            .unwrap();
        let other = snapshot_worker_dir(&root, 9);
        mudu_sys::fs::sync::create_dir_all(&other).unwrap();
        mudu_sys::fs::sync::write(other.join(PENDING), b"invalid").unwrap();
        assert!(recover_pending_installs(&root, &root, 4096, &[identity()])
            .await
            .is_err());
        assert_eq!(mudu_sys::fs::sync::read(stale).unwrap(), b"retain");
        assert!(super::super::base::read(&snapshot_worker_dir(&root, 0))
            .unwrap()
            .is_none());
    })
    .unwrap();
}

#[test]
fn advanced_gc_or_corrupt_source_keeps_pending_and_live_files() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for corrupt_source in [false, true] {
            let (root, manifest) = tests::source().await;
            let relation = Path::new(&root).join("relation");
            mudu_sys::fs::sync::create_dir_all(&relation).unwrap();
            let stale = relation.join("0.1.dat");
            mudu_sys::fs::sync::write(&stale, b"retain").unwrap();
            let _journal = InstallJournal::begin(&root, 42, &[0], &manifest)
                .await
                .unwrap();
            if corrupt_source {
                mudu_sys::fs::sync::write(
                    snapshot_version_dir(&root, 0, 10).join(KV_FILE_NAME),
                    b"bad",
                )
                .unwrap();
            } else {
                wal::worker_log::WorkerLogLayout::new(&root, 42, 4096)
                    .unwrap()
                    .store_gc_frontier(wal::worker_log::GcFrontier {
                        lsn: 12,
                        first_alive_seq: 0,
                    })
                    .unwrap();
            }
            assert!(recover_pending_installs(&root, &root, 4096, &[identity()])
                .await
                .is_err());
            assert_eq!(mudu_sys::fs::sync::read(stale).unwrap(), b"retain");
            assert!(ensure_no_pending_installs(&root).is_err());
        }
    })
    .unwrap();
}
