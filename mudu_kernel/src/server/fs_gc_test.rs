#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Tests for [`super::fs_gc::FsGc`].
//!
//! The content filesystem is the in-memory [`MemFs`] and `_fs_object` rows
//! come from the [`MockFsObjectStore`] map, both shared with the
//! [`super::fs_service`] tests. The horizon source is a real
//! [`WorkerXContract`] over the mock meta manager; its snapshot manager is
//! in-memory, so no storage IO is involved.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mudu::common::id::OID;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::sync::async_::stop_flag::stop_channel;
use mudu_sys::task::async_::{block_on_tokio_current_thread, sleep, spawn_local_task, timeout};
use mudu_utils::notifier::notify_wait;

use crate::contract::fs_type::FsTypeKind;
use crate::meta::fs_type_catalog::fs_storage_root;
use crate::server::fs_gc::FsGc;
use crate::server::fs_service_test::{
    block_on, pending_row, sealed_row, FsTestMetaMgr, MemFs, MockFsObjectStore, DIR_FS_ID,
    FILE_FS_ID,
};
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::server::x_contract::WorkerXContract;

struct GcFixture {
    gc: FsGc,
    fs: Arc<MemFs>,
    store: Arc<MockFsObjectStore>,
    data_dir: String,
}

fn new_gc() -> GcFixture {
    let data_dir = "/memfs-gc-data".to_string();
    let fs = Arc::new(MemFs::new());
    let store = Arc::new(MockFsObjectStore::new());
    let meta_mgr = Arc::new(FsTestMetaMgr::new());
    let snapshot_source = Arc::new(WorkerXContract::new(meta_mgr.clone()).unwrap());
    let gc = FsGc::new(
        data_dir.clone(),
        fs.clone(),
        meta_mgr,
        store.clone(),
        snapshot_source,
    );
    GcFixture {
        gc,
        fs,
        store,
        data_dir,
    }
}

/// Flat-layout host path, mirroring the one in `fs_service_test`.
fn content_path(data_dir: &str, fs_id: u64, oid: OID, generation: u64, entry_rel: &str) -> PathBuf {
    let mut components = entry_rel.split('/');
    let mut name = format!("{oid:032x}.{generation}");
    if let Some(first) = components.next().filter(|first| !first.is_empty()) {
        name.push('.');
        name.push_str(first);
    }
    components.fold(
        fs_storage_root(data_dir, fs_id).join(name),
        |path, component| path.join(component),
    )
}

#[test]
fn recover_scan_keeps_sealed_generations_and_reclaims_orphans() {
    block_on(async {
        let f = new_gc();
        let sealed_file_oid = 0xF500_0000_0000_0101u128;
        let sealed_dir_oid = 0xF500_0000_0000_0102u128;
        let orphan_oid = 0xF500_0000_0000_0103u128;
        let pending_oid = 0xF500_0000_0000_0104u128;

        // (a) Live generations of sealed rows: a FILE object at generation 2
        // and a DIRECTORY object with nested entries at generation 1.
        f.store.insert(
            sealed_file_oid,
            0,
            sealed_row(FILE_FS_ID, FsTypeKind::File, 2, 5),
            false,
        );
        let live_file = content_path(&f.data_dir, FILE_FS_ID, sealed_file_oid, 2, "");
        f.fs.put_file(&live_file, b"hello");
        f.store.insert(
            sealed_dir_oid,
            0,
            sealed_row(DIR_FS_ID, FsTypeKind::Directory, 1, 0),
            false,
        );
        let live_entry = content_path(&f.data_dir, DIR_FS_ID, sealed_dir_oid, 1, "x.txt");
        let live_nested = content_path(&f.data_dir, DIR_FS_ID, sealed_dir_oid, 1, "a/b.txt");
        f.fs.put_file(&live_entry, b"x");
        f.fs.put_file(&live_nested, b"y");

        // (c) An older generation of the sealed FILE object: the row points
        // at generation 2, so generation 1 is reclaimed.
        let stale_file = content_path(&f.data_dir, FILE_FS_ID, sealed_file_oid, 1, "");
        f.fs.put_file(&stale_file, b"old");

        // (b) A generation with no object row at all, including a nested
        // DIRECTORY subtree that must disappear recursively.
        let orphan_file = content_path(&f.data_dir, FILE_FS_ID, orphan_oid, 1, "");
        let orphan_nested = content_path(&f.data_dir, DIR_FS_ID, orphan_oid, 1, "sub/c.txt");
        f.fs.put_file(&orphan_file, b"orphan");
        f.fs.put_file(&orphan_nested, b"orphan");

        // (e) Generations whose row never committed past PENDING: neither
        // the written generation 1 nor the pending generation 0 is sealed.
        f.store.insert(
            pending_oid,
            0,
            pending_row(FILE_FS_ID, FsTypeKind::File),
            false,
        );
        let pending_file = content_path(&f.data_dir, FILE_FS_ID, pending_oid, 1, "");
        let pending_gen0 = content_path(&f.data_dir, FILE_FS_ID, pending_oid, 0, "");
        f.fs.put_file(&pending_file, b"pending");
        f.fs.put_file(&pending_gen0, b"p0");

        // (d) A whole fs id directory missing from the catalog.
        let dropped_root = fs_storage_root(&f.data_dir, 99);
        let dropped_file = dropped_root.join(format!("{orphan_oid:032x}.1"));
        f.fs.put_file(&dropped_file, b"dropped");

        // Names the flat layout never produces are left alone.
        let foreign = fs_storage_root(&f.data_dir, FILE_FS_ID).join("README.txt");
        f.fs.put_file(&foreign, b"keep me");

        f.gc.recover_scan().await.unwrap();

        assert_eq!(f.fs.read_file(&live_file).unwrap(), b"hello".to_vec());
        assert_eq!(f.fs.read_file(&live_entry).unwrap(), b"x".to_vec());
        assert_eq!(f.fs.read_file(&live_nested).unwrap(), b"y".to_vec());
        assert_eq!(f.fs.read_file(&foreign).unwrap(), b"keep me".to_vec());

        assert!(!f.fs.path_exists(&stale_file).await.unwrap());
        assert!(!f.fs.path_exists(&orphan_file).await.unwrap());
        assert!(!f.fs.path_exists(&orphan_nested).await.unwrap());
        assert!(!f.fs.path_exists(&pending_file).await.unwrap());
        assert!(!f.fs.path_exists(&pending_gen0).await.unwrap());
        assert!(!f.fs.path_exists(&dropped_root).await.unwrap());
    });
}

#[test]
fn recover_scan_with_no_fs_base_directory_is_a_noop() {
    block_on(async {
        let f = new_gc();
        f.gc.recover_scan().await.unwrap();
    });
}

#[test]
fn gc_once_reclaims_generations_past_the_horizon() {
    block_on(async {
        let f = new_gc();
        let visible_oid = 0xF500_0000_0000_0201u128;
        let deleted_oid = 0xF500_0000_0000_0202u128;
        let staged_oid = 0xF500_0000_0000_0203u128;

        // Object visible at the horizon: the row's current generation is
        // kept, every other generation of the same object is reclaimed.
        f.store.insert(
            visible_oid,
            0,
            sealed_row(FILE_FS_ID, FsTypeKind::File, 2, 5),
            false,
        );
        let current = content_path(&f.data_dir, FILE_FS_ID, visible_oid, 2, "");
        let older = content_path(&f.data_dir, FILE_FS_ID, visible_oid, 1, "");
        let superseded = content_path(&f.data_dir, FILE_FS_ID, visible_oid, 3, "");
        f.fs.put_file(&current, b"new");
        f.fs.put_file(&older, b"old");
        f.fs.put_file(&superseded, b"stale");

        // Object with no committed row at the horizon (deleted or never
        // committed): all of its generations are reclaimed.
        let gone_gen1 = content_path(&f.data_dir, FILE_FS_ID, deleted_oid, 1, "");
        let gone_gen2 = content_path(&f.data_dir, FILE_FS_ID, deleted_oid, 2, "");
        f.fs.put_file(&gone_gen1, b"g1");
        f.fs.put_file(&gone_gen2, b"g2");

        // A staged-only row is not visible to a committed-only read.
        f.store.insert(
            staged_oid,
            0,
            pending_row(FILE_FS_ID, FsTypeKind::File),
            true,
        );
        let staged_file = content_path(&f.data_dir, FILE_FS_ID, staged_oid, 1, "");
        f.fs.put_file(&staged_file, b"staged");

        // A dropped fs id loses its whole storage root here too.
        let dropped_root = fs_storage_root(&f.data_dir, 99);
        let dropped_file = dropped_root.join(format!("{deleted_oid:032x}.1"));
        f.fs.put_file(&dropped_file, b"dropped");

        // Names the flat layout never produces are left alone.
        let foreign = fs_storage_root(&f.data_dir, FILE_FS_ID).join("README.txt");
        f.fs.put_file(&foreign, b"keep me");

        let horizon = WorkerSnapshot::new(100, Vec::new());
        f.gc.gc_once(&horizon).await.unwrap();

        assert_eq!(f.fs.read_file(&current).unwrap(), b"new".to_vec());
        assert_eq!(f.fs.read_file(&foreign).unwrap(), b"keep me".to_vec());

        assert!(!f.fs.path_exists(&older).await.unwrap());
        assert!(!f.fs.path_exists(&superseded).await.unwrap());
        assert!(!f.fs.path_exists(&gone_gen1).await.unwrap());
        assert!(!f.fs.path_exists(&gone_gen2).await.unwrap());
        assert!(!f.fs.path_exists(&staged_file).await.unwrap());
        assert!(!f.fs.path_exists(&dropped_root).await.unwrap());
    });
}

#[test]
fn gc_loop_exits_promptly_on_stop() {
    block_on_tokio_current_thread(async move {
        let f = new_gc();
        let (stop_tx, stop_rx) = stop_channel();
        let (_notifier, waiter) = notify_wait();
        let gc = f.gc;
        let join = spawn_local_task(waiter, "fs_gc_loop_test", async move {
            gc.gc_loop(Duration::from_millis(10), stop_rx).await
        })
        .unwrap();
        sleep(Duration::from_millis(50)).await.unwrap();
        assert!(!join.is_finished());
        stop_tx.stop();
        let joined = timeout(Duration::from_secs(5), join)
            .await
            .expect("fs gc loop did not exit promptly")
            .unwrap()
            .expect("fs gc loop task was canceled unexpectedly");
        joined.unwrap();
    })
    .unwrap();
}
