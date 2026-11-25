use super::*;
use crate::snapshot::{recover_worker_storage, recovery_base_resume, take_worker_snapshot};

fn root() -> std::path::PathBuf {
    mudu_sys::env_var::temp_dir().join(format!("snapshot-base-{}", mudu_utils::oid::gen_oid()))
}

async fn commit(worker: &WorkerRuntime, xid: u64, key: &[u8], value: &[u8]) -> u64 {
    let (_, last, frames) = produce_kv_segment(worker, xid, key, value).await;
    for batch in
        wal::xl_batch::decode_xl_batches_with_pending(&frames, &mut vec![], &mut None).unwrap()
    {
        worker.replay_log_batch_at(batch, last + 1).await.unwrap();
    }
    last
}

#[test]
fn restart_restores_base_and_only_replays_the_retained_suffix() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = root();
        let worker = build_worker(&dir).await;
        let value = vec![7; 24_000];
        let cut = commit(&worker, 1001, b"base", &value).await;
        take_worker_snapshot(&worker, 0, cut, [0; 32], 0, 0)
            .await
            .unwrap();
        let tail = commit(&worker, 1002, b"suffix", b"new").await;
        let log = worker.worker_log().unwrap().unwrap();
        let (_, removed) = log
            .layout()
            .unwrap()
            .gc_prefix_chunks(log.fs().as_ref(), cut + 1)
            .await
            .unwrap();
        assert!(
            removed > 0,
            "test must really reclaim the covered WAL prefix"
        );
        drop(log);
        drop(worker);
        let reopened = build_worker(&dir).await;
        assert!(reopened.storage().dump_kv_latest().await.is_empty());
        recover_worker_storage(&reopened).await.unwrap();
        assert_eq!(reopened.get_async(b"base").await.unwrap(), Some(value));
        assert_eq!(
            reopened.get_async(b"suffix").await.unwrap(),
            Some(b"new".to_vec())
        );
        assert_eq!(reopened.worker_x_contract().latest_xid(), 1002);
        assert_eq!(recovery_base_resume(&reopened).unwrap(), Some(cut + 1));
        assert_eq!(
            reopened.worker_log().unwrap().unwrap().next_lsn().as_u64(),
            tail + 1
        );
    })
    .unwrap();
}

#[test]
fn base_without_wal_seeds_allocator_and_preserves_data_across_restart() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = root();
        let worker = build_worker(&dir).await;
        let cut = commit(&worker, 1001, b"base", b"durable").await;
        take_worker_snapshot(&worker, 0, cut, [0; 32], 0, 0)
            .await
            .unwrap();
        let log = worker.worker_log().unwrap().unwrap();
        let paths = log.chunk_paths_sorted().await.unwrap();
        drop(log);
        drop(worker);
        for path in paths {
            mudu_sys::fs::sync::sync_remove_file(path).unwrap();
        }
        let reopened = build_worker(&dir).await;
        recover_worker_storage(&reopened).await.unwrap();
        assert_eq!(
            reopened.worker_log().unwrap().unwrap().next_lsn().as_u64(),
            cut + 1
        );
        assert_eq!(
            reopened.get_async(b"base").await.unwrap(),
            Some(b"durable".to_vec())
        );
        let (resume, pending, _) =
            crate::scan_worker_log_tail(&reopened.worker_log().unwrap().unwrap(), cut + 1)
                .await
                .unwrap();
        assert_eq!(resume, cut + 1);
        assert!(pending.is_empty());
        let tail = commit(&reopened, 1002, b"later", b"value").await;
        assert!(tail > cut);
        drop(reopened);
        let again = build_worker(&dir).await;
        recover_worker_storage(&again).await.unwrap();
        assert_eq!(
            again.get_async(b"base").await.unwrap(),
            Some(b"durable".to_vec())
        );
        assert_eq!(
            again.get_async(b"later").await.unwrap(),
            Some(b"value".to_vec())
        );
    })
    .unwrap();
}

#[test]
fn recovery_rejects_a_gap_after_the_snapshot_cut() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = root();
        let worker = build_worker(&dir).await;
        let cut = commit(&worker, 1001, b"base", b"durable").await;
        take_worker_snapshot(&worker, 0, cut, [0; 32], 0, 0)
            .await
            .unwrap();
        let log = worker.worker_log().unwrap().unwrap();
        let next = std::sync::atomic::AtomicU64::new(cut + 2);
        let frames = wal::xl_batch_worker_log::serialize_batch(
            &XLBatch::new(vec![XLEntry {
                xid: 1002,
                ops: vec![],
                commit_epoch: None,
            }]),
            4096,
            &next,
        )
        .unwrap();
        log.append_frames_async(frames).await.unwrap();
        log.flush_async().await.unwrap();
        drop(log);
        drop(worker);
        let reopened = build_worker(&dir).await;
        assert!(recover_worker_storage(&reopened)
            .await
            .unwrap_err()
            .message()
            .contains("gap"));
    })
    .unwrap();
}

#[test]
fn foreign_or_corrupt_base_cannot_seed_a_worker() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = root();
        let worker = build_worker(&dir).await;
        let cut = commit(&worker, 1001, b"base", b"durable").await;
        take_worker_snapshot(&worker, 0, cut, [0; 32], 0, 0)
            .await
            .unwrap();
        let base_path = dir.join("data/snapshot/worker_0/recovery.base.mp");
        let bytes = mudu_sys::fs::sync::read(&base_path).unwrap();
        let other_dir = root();
        let other = build_worker(&other_dir).await;
        let dest = other_dir.join("data/snapshot/worker_0");
        mudu_sys::fs::sync::create_dir_all(&dest).unwrap();
        mudu_sys::fs::sync::write(dest.join("recovery.base.mp"), &bytes).unwrap();
        assert!(recover_worker_storage(&other).await.is_err());
        assert!(other.storage().dump_kv_latest().await.is_empty());
        drop(worker);
        mudu_sys::fs::sync::write(base_path, b"corrupt").unwrap();
        let reopened = build_worker(&dir).await;
        assert!(recover_worker_storage(&reopened).await.is_err());
        assert!(reopened.storage().dump_kv_latest().await.is_empty());
    })
    .unwrap();
}

#[test]
fn sealed_download_without_publication_is_not_a_recovery_base() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let source_dir = root();
        let source = build_worker(&source_dir).await;
        let cut = commit(&source, 1001, b"foreign", b"not-installed").await;
        let manifest = take_worker_snapshot(&source, 0, cut, [0; 32], 0, 0)
            .await
            .unwrap();
        let target_dir = root();
        let target = build_worker(&target_dir).await;
        let from = crate::snapshot::snapshot_version_dir(source.storage().relation_path(), 0, cut);
        let to = crate::snapshot::snapshot_version_dir(target.storage().relation_path(), 0, cut);
        for entry in &manifest.files {
            let dest = to.join(&entry.path);
            mudu_sys::fs::sync::create_dir_all(dest.parent().unwrap()).unwrap();
            mudu_sys::fs::sync::write(
                dest,
                mudu_sys::fs::sync::read(from.join(&entry.path)).unwrap(),
            )
            .unwrap();
        }
        crate::snapshot::seal_snapshot(&to, &manifest)
            .await
            .unwrap();
        recover_worker_storage(&target).await.unwrap();
        assert_eq!(recovery_base_resume(&target).unwrap(), None);
        assert!(target.storage().dump_kv_latest().await.is_empty());
    })
    .unwrap();
}

#[test]
fn recovery_rejects_gc_beyond_the_published_base() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = root();
        let worker = build_worker(&dir).await;
        let cut = commit(&worker, 1001, b"base", b"durable").await;
        take_worker_snapshot(&worker, 0, cut, [0; 32], 0, 0)
            .await
            .unwrap();
        worker
            .worker_log()
            .unwrap()
            .unwrap()
            .layout()
            .unwrap()
            .store_gc_frontier(wal::worker_log::GcFrontier {
                lsn: cut + 2,
                first_alive_seq: 0,
            })
            .unwrap();
        drop(worker);
        let reopened = build_worker(&dir).await;
        assert!(recover_worker_storage(&reopened)
            .await
            .unwrap_err()
            .message()
            .contains("GC"));
        assert!(reopened.storage().dump_kv_latest().await.is_empty());
    })
    .unwrap();
}

#[test]
fn interrupted_install_redo_discards_tail_before_worker_recovery() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = root();
        let worker = build_worker(&dir).await;
        let cut = commit(&worker, 2001, b"base", b"durable").await;
        let manifest = take_worker_snapshot(&worker, 0, cut, [0; 32], 0, 0)
            .await
            .unwrap();
        commit(&worker, 2002, b"speculative", b"discard").await;
        let data = worker.storage().relation_path().to_owned();
        let target = worker::worker_registry::WorkerIdobject {
            worker_index: 0,
            worker_id: worker.worker_id(),
            partition_ids: worker.partition_ids(),
        };
        let journal = crate::snapshot::InstallJournal::begin(
            &data,
            worker.worker_id(),
            &worker.partition_ids(),
            &manifest,
        )
        .await
        .unwrap();
        drop(journal);
        drop(worker);
        crate::snapshot::recover_pending_installs(
            &data,
            dir.join("wal").to_str().unwrap(),
            4096,
            &[target],
        )
        .await
        .unwrap();
        let reopened = build_worker(&dir).await;
        recover_worker_storage(&reopened).await.unwrap();
        assert_eq!(
            reopened.get_async(b"base").await.unwrap(),
            Some(b"durable".to_vec())
        );
        assert_eq!(reopened.get_async(b"speculative").await.unwrap(), None);
        assert_eq!(
            reopened.worker_log().unwrap().unwrap().next_lsn().as_u64(),
            cut + 1
        );
        commit(&reopened, 2003, b"after", b"redo").await;
        drop(reopened);
        let reopened = build_worker(&dir).await;
        recover_worker_storage(&reopened).await.unwrap();
        assert_eq!(
            reopened.get_async(b"after").await.unwrap(),
            Some(b"redo".to_vec())
        );
        assert_eq!(reopened.get_async(b"speculative").await.unwrap(), None);
    })
    .unwrap();
}
