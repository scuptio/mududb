#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use catalog::contract::{
    snapshot::{RunningXList, Snapshot},
    timestamp::Timestamp,
    version_tuple::VersionTuple,
};
use mudu_sys::contract::file_options::FileOptions;

#[tokio::test]
async fn cold_rows_preserve_history_and_live_clones_pin_shared_identity() {
    let fs = mudu_sys::default_sys_io_context().fs();
    let path = mudu_sys::env_var::temp_dir()
        .join(format!("btree-row-cache-{}", mudu_utils::oid::gen_oid()));
    fs.create_dir_all(&path).await.unwrap();
    let data = fs
        .open(&path.join("rows"), FileOptions::read_write_create())
        .await
        .unwrap();
    let dir = fs
        .open(&path.join("directory"), FileOptions::read_write_create())
        .await
        .unwrap();
    let mut cache = Rows::new(data, dir).await.unwrap();
    cache.budget = 256;
    let row = DataRow::new(1);
    for xid in 1..=6 {
        row.write(
            VersionTuple::new(Timestamp::new(xid, u64::MAX), vec![xid as u8; 200]),
            None,
        )
        .await
        .unwrap();
    }
    cache.put(row).await.unwrap();
    assert!(!cache.shards[1].lock().await.rows.contains_key(&1));
    let live = cache.get(1).await.unwrap();
    assert_eq!(
        live.read(&Snapshot::from(RunningXList::new(1, vec![])))
            .await
            .unwrap()
            .unwrap()
            .tuple(),
        &vec![1; 200]
    );
    live.write_shallow(VersionTuple::new(Timestamp::new(7, u64::MAX), vec![7; 200]))
        .await
        .unwrap();
    cache.put(DataRow::new(17)).await.unwrap();
    assert!(cache.shards[1].lock().await.rows.contains_key(&1));
    let same = cache.get(1).await.unwrap();
    assert_eq!(
        same.read_latest().await.unwrap().unwrap().tuple(),
        &vec![7; 200]
    );
    drop(same);
    drop(live);
    cache.put(DataRow::new(33)).await.unwrap();
    assert!(!cache.shards[1].lock().await.rows.contains_key(&1));
    let reloaded = cache.get(1).await.unwrap();
    assert_eq!(
        reloaded.read_latest().await.unwrap().unwrap().tuple(),
        &vec![7; 200]
    );
    cache.flush().await.unwrap();
}

#[tokio::test]
async fn eviction_after_checkpoint_preserves_durable_extents() {
    let fs = mudu_sys::default_sys_io_context().fs();
    let path = mudu_sys::env_var::temp_dir().join(format!(
        "btree-row-checkpoint-{}",
        mudu_utils::oid::gen_oid()
    ));
    fs.create_dir_all(&path).await.unwrap();
    let data = fs
        .open(&path.join("rows"), FileOptions::read_write_create())
        .await
        .unwrap();
    let dir = fs
        .open(&path.join("directory"), FileOptions::read_write_create())
        .await
        .unwrap();
    let mut cache = Rows::new(data.clone(), dir.clone()).await.unwrap();
    let row = DataRow::new(1);
    row.write_shallow(VersionTuple::new(Timestamp::new(1, u64::MAX), vec![1; 200]))
        .await
        .unwrap();
    cache.put(row).await.unwrap();
    cache.flush().await.unwrap();
    let len = data.file_len().await.unwrap();
    let directory = dir.read_exact_at(24, 24).await.unwrap();
    cache.budget = 0;
    cache
        .trim(&mut *cache.shards[1].lock().await)
        .await
        .unwrap();
    assert!(cache.shards[1].lock().await.rows.is_empty());
    assert_eq!(data.file_len().await.unwrap(), len);
    assert_eq!(dir.read_exact_at(24, 24).await.unwrap(), directory);
    let loaded = cache.get(1).await.unwrap();
    assert_eq!(
        loaded.read_latest().await.unwrap().unwrap().tuple(),
        &vec![1; 200]
    );
}

struct FailAfterWrite {
    file: Arc<dyn AsyncFile>,
    fail: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl AsyncFile for FailAfterWrite {
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        self.file.read_exact_at(offset, len).await
    }
    async fn write_all_at(&self, offset: u64, bytes: &[u8]) -> RS<()> {
        self.file.write_all_at(offset, bytes).await?;
        if self.fail.load(Ordering::Acquire) {
            return Err(error("injected post-write failure"));
        }
        Ok(())
    }
    async fn fsync(&self) -> RS<()> {
        self.file.fsync().await
    }
    async fn file_len(&self) -> RS<u64> {
        self.file.file_len().await
    }
}

#[tokio::test]
async fn failed_directory_publication_forgets_previous_digest() {
    let fs = mudu_sys::default_sys_io_context().fs();
    let path = mudu_sys::env_var::temp_dir()
        .join(format!("btree-row-retry-{}", mudu_utils::oid::gen_oid()));
    fs.create_dir_all(&path).await.unwrap();
    let data = fs
        .open(&path.join("rows"), FileOptions::read_write_create())
        .await
        .unwrap();
    let dir = fs
        .open(&path.join("directory"), FileOptions::read_write_create())
        .await
        .unwrap();
    let faulty = Arc::new(FailAfterWrite {
        file: dir,
        fail: false.into(),
    });
    let cache = Rows::new(data.clone(), faulty.clone()).await.unwrap();
    let row = DataRow::new(1);
    row.write_shallow(VersionTuple::new(Timestamp::new(1, u64::MAX), vec![1; 100]))
        .await
        .unwrap();
    let original = row.export_snapshot().await.unwrap();
    cache.put(row.clone()).await.unwrap();
    cache.flush().await.unwrap();
    row.write_shallow(VersionTuple::new(Timestamp::new(2, u64::MAX), vec![2; 100]))
        .await
        .unwrap();
    faulty.fail.store(true, Ordering::Release);
    assert!(cache.flush().await.is_err());
    faulty.fail.store(false, Ordering::Release);
    cache
        .put(DataRow::restore_snapshot(&original).unwrap())
        .await
        .unwrap();
    cache.flush().await.unwrap();
    let reopened = Rows::new(data, faulty).await.unwrap();
    assert_eq!(
        reopened
            .get(1)
            .await
            .unwrap()
            .export_snapshot()
            .await
            .unwrap(),
        original
    );
}
