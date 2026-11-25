//! Injected durability boundaries over real Tokio files.
#![allow(clippy::unwrap_used, clippy::panic)]
use super::*;
use async_trait::async_trait;
use mudu_sys::contract::async_file::AsyncFile;
use std::sync::atomic::AtomicU64;
use tokio::sync::Notify;

#[derive(Default)]
struct Probe {
    synced: SMutex<Vec<PathBuf>>,
    fail_sync: AtomicBool,
    pause_sync: AtomicBool,
    entered: Notify,
    release: Notify,
}
struct ProbeFs {
    inner: Arc<dyn AsyncFs>,
    probe: Arc<Probe>,
}
struct ProbeFile {
    inner: Arc<dyn AsyncFile>,
    path: PathBuf,
    probe: Arc<Probe>,
}
#[async_trait]
impl AsyncFile for ProbeFile {
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        self.inner.read_exact_at(offset, len).await
    }
    async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        self.inner.write_all_at(offset, payload).await
    }
    async fn fsync(&self) -> RS<()> {
        self.probe.synced.lock().unwrap().push(self.path.clone());
        if self.probe.pause_sync.load(Ordering::Acquire) {
            self.probe.entered.notify_one();
            self.probe.release.notified().await;
        }
        if self.probe.fail_sync.load(Ordering::Acquire) {
            return Err(mudu_error!(ErrorCode::Io, "injected fsync failure"));
        }
        self.inner.fsync().await
    }
    async fn set_len(&self, len: u64) -> RS<()> {
        self.inner.set_len(len).await
    }
    async fn file_len(&self) -> RS<u64> {
        self.inner.file_len().await
    }
}
#[async_trait]
impl AsyncFs for ProbeFs {
    async fn open(&self, path: &Path, options: FileOptions) -> RS<Arc<dyn AsyncFile>> {
        Ok(Arc::new(ProbeFile {
            inner: self.inner.open(path, options).await?,
            path: path.to_path_buf(),
            probe: self.probe.clone(),
        }))
    }
    async fn create_dir_all(&self, path: &Path) -> RS<()> {
        self.inner.create_dir_all(path).await
    }
    async fn metadata_len(&self, path: &Path) -> RS<u64> {
        self.inner.metadata_len(path).await
    }
    async fn path_exists(&self, path: &Path) -> RS<bool> {
        self.inner.path_exists(path).await
    }
    async fn remove_file_if_exists(&self, path: &Path) -> RS<()> {
        self.inner.remove_file_if_exists(path).await
    }
    async fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>> {
        self.inner.read_dir(path).await
    }
    async fn rename(&self, from: &Path, to: &Path) -> RS<()> {
        self.inner.rename(from, to).await
    }
    async fn sync_dir(&self, path: &Path) -> RS<()> {
        self.probe.synced.lock().unwrap().push(path.to_path_buf());
        self.inner.sync_dir(path).await
    }
}
async fn store() -> (StagingStore, Arc<Probe>) {
    let dir = mudu_sys::env_var::temp_dir()
        .join(format!("staging-proof-{}", mudu_sys::random::uuid_v4()));
    let probe = Arc::new(Probe::default());
    let fs = Arc::new(ProbeFs {
        inner: mudu_sys::fs::tokio_fs(),
        probe: probe.clone(),
    });
    (StagingStore::open(dir, fs).await.unwrap(), probe)
}
fn frame(lsn: u64) -> Vec<Vec<u8>> {
    wal::log_frame::serialize_entry(&lsn, 4096, &AtomicU64::new(lsn)).unwrap()
}

#[tokio::test]
async fn flush_syncs_all_rotated_dirty_chunks_before_directory() {
    let (store, probe) = store().await;
    store.append_frames(&frame(0)).await.unwrap();
    // Force the next append to rotate without allocating a 16 MiB test payload.
    store.lock_index().unwrap().tail.as_mut().unwrap().1 = STAGING_CHUNK_SIZE;
    store.append_frames(&frame(1)).await.unwrap();
    probe.synced.lock().unwrap().clear();
    store.flush().await.unwrap();
    assert_eq!(
        *probe.synced.lock().unwrap(),
        vec![store.chunk_path(0), store.chunk_path(1), store.dir.clone()]
    );
    assert!(store.lock_index().unwrap().dirty.is_empty());
    mudu_sys::fs::tokio_fs()
        .remove_dir_all(&store.dir)
        .await
        .unwrap();
}

#[tokio::test]
async fn failed_fsync_retains_dirty_state_and_refuses_further_admission() {
    let (store, probe) = store().await;
    store.append_frames(&frame(0)).await.unwrap();
    probe.fail_sync.store(true, Ordering::Release);
    assert!(store.flush().await.is_err());
    assert!(!store.lock_index().unwrap().dirty.is_empty());
    assert!(store.append_frames(&frame(1)).await.is_err());
    assert!(store.drain().await.is_err());
    mudu_sys::fs::tokio_fs()
        .remove_dir_all(&store.dir)
        .await
        .unwrap();
}

#[tokio::test]
async fn cancellation_does_not_release_io_owner_until_real_fsync_finishes() {
    let (store, probe) = store().await;
    store.append_frames(&frame(0)).await.unwrap();
    probe.pause_sync.store(true, Ordering::Release);
    let mut flush = Box::pin(store.flush());
    tokio::select! { _ = &mut flush => panic!("fsync should be held"), _ = probe.entered.notified() => {} }
    drop(flush);
    assert!(
        mudu_sys::timeout(std::time::Duration::from_millis(5), store.drain())
            .await
            .is_none()
    );
    probe.pause_sync.store(false, Ordering::Release);
    probe.release.notify_one();
    store.drain().await.unwrap();
    store.append_frames(&frame(1)).await.unwrap();
    store.flush().await.unwrap();
    mudu_sys::fs::tokio_fs()
        .remove_dir_all(&store.dir)
        .await
        .unwrap();
}

#[tokio::test]
async fn missing_dirty_chunk_cannot_be_recreated_and_acknowledged_empty() {
    let (store, _) = store().await;
    store.append_frames(&frame(0)).await.unwrap();
    mudu_sys::fs::tokio_fs()
        .remove_file_if_exists(&store.chunk_path(0))
        .await
        .unwrap();
    assert!(store.flush().await.is_err());
    assert!(!mudu_sys::fs::tokio_fs()
        .path_exists(&store.chunk_path(0))
        .await
        .unwrap());
    assert!(store.drain().await.is_err());
    mudu_sys::fs::tokio_fs()
        .remove_dir_all(&store.dir)
        .await
        .unwrap();
}
