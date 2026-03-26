use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio::sync::{Mutex, MutexGuard};

pub const META_FILE_NAME: &str = "linear_hash.meta.json";
pub const PAGE_FILE_NAME: &str = "linear_hash.pages";

const META_VERSION: u32 = 1;
const PAGE_HEADER_SIZE: usize = 16;
const NONE_PAGE_ID: u64 = u64::MAX;

#[derive(Debug, Clone)]
pub struct LinearHashConfig {
    pub page_size: usize,
    pub key_size: usize,
    pub value_size: usize,
    pub target_load_factor: f64,
    pub initial_buckets: u32,
}

impl Default for LinearHashConfig {
    fn default() -> Self {
        Self {
            page_size: 4096,
            key_size: 32,
            value_size: 128,
            target_load_factor: 0.80,
            initial_buckets: 4,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LinearHashOpenOptions {
    pub create_if_missing: bool,
    pub create_new: bool,
}

impl Default for LinearHashOpenOptions {
    fn default() -> Self {
        Self {
            create_if_missing: true,
            create_new: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LinearHashMeta {
    version: u32,
    page_size: usize,
    key_size: usize,
    value_size: usize,
    bucket_capacity: usize,
    target_load_factor: f64,
    level: u32,
    split_pointer: u32,
    bucket_page_ids: Vec<u64>,
    next_page_id: u64,
    free_page_ids: Vec<u64>,
    total_entries: u64,
}

#[derive(Debug, Clone)]
struct BucketPage {
    next_page_id: Option<u64>,
    entries: Vec<(Vec<u8>, Vec<u8>)>,
}

struct DirectoryState {
    meta: LinearHashMeta,
    bucket_locks: Vec<Arc<Mutex<()>>>,
}

pub struct LinearHash {
    dir: PathBuf,
    directory: Mutex<DirectoryState>,
}

impl LinearHash {
    pub async fn open<P: AsRef<Path>>(
        dir: P,
        config: LinearHashConfig,
        options: LinearHashOpenOptions,
    ) -> RS<Self> {
        config.validate()?;
        let dir = dir.as_ref().to_path_buf();
        let meta_path = dir.join(META_FILE_NAME);
        let page_path = dir.join(PAGE_FILE_NAME);
        let meta_exists = meta_path.exists();
        let page_exists = page_path.exists();

        if options.create_new && (meta_exists || page_exists) {
            return Err(m_error!(
                ER::ExistingSuchElement,
                "linear hash already exists for this path"
            ));
        }

        if !meta_exists || !page_exists {
            if !options.create_if_missing {
                return Err(m_error!(
                    ER::NoSuchElement,
                    "linear hash files not found and create_if_missing=false"
                ));
            }
            fs::create_dir_all(&dir)
                .await
                .map_err(|e| m_error!(ER::IOErr, "create index dir error", e))?;
            return Self::create_new(dir, config).await;
        }

        let meta = Self::read_meta(&meta_path).await?;
        Self::validate_meta_compat(&meta, &config)?;
        let bucket_locks = (0..meta.bucket_page_ids.len())
            .map(|_| Arc::new(Mutex::new(())))
            .collect::<Vec<_>>();

        Ok(Self {
            dir,
            directory: Mutex::new(DirectoryState { meta, bucket_locks }),
        })
    }

    pub async fn get(&self, key: &[u8], _optional_argv: Option<&[u8]>) -> RS<Option<Vec<u8>>> {
        let (meta, bucket_index, bucket_lock) = self.lock_bucket_for_key(key).await?;
        let _bucket_guard = bucket_lock.lock().await;
        let entries = self.read_bucket_entries(&meta, bucket_index).await?;
        for (k, v) in entries {
            if k == key {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    pub async fn set(&self, key: &[u8], value: &[u8], _optional_argv: Option<&[u8]>) -> RS<()> {
        let mut dir = self.directory.lock().await;
        validate_key_len(&dir.meta, key)?;
        validate_value_len(&dir.meta, value)?;
        let bucket_index = bucket_index_for_key(&dir.meta, key);
        let bucket_lock = dir.bucket_locks.get(bucket_index).cloned().ok_or_else(|| {
            m_error!(
                ER::IndexOutOfRange,
                format!("invalid bucket {}", bucket_index)
            )
        })?;

        let inserted = {
            let _bucket_guard = bucket_lock.lock().await;
            let mut entries = self.read_bucket_entries(&dir.meta, bucket_index).await?;
            let mut replaced = false;
            for (k, v) in &mut entries {
                if k.as_slice() == key {
                    *v = value.to_vec();
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                entries.push((key.to_vec(), value.to_vec()));
            }
            self.rewrite_bucket(&mut dir.meta, bucket_index, &entries)
                .await?;
            !replaced
        };

        if inserted {
            dir.meta.total_entries += 1;
        }
        self.expand_if_needed_locked(&mut dir).await?;
        self.write_meta_file(&self.meta_path(), &dir.meta).await?;
        Ok(())
    }

    pub async fn delete(&self, key: &[u8], _optional_argv: Option<&[u8]>) -> RS<bool> {
        let mut dir = self.directory.lock().await;
        validate_key_len(&dir.meta, key)?;
        let bucket_index = bucket_index_for_key(&dir.meta, key);
        let bucket_lock = dir.bucket_locks.get(bucket_index).cloned().ok_or_else(|| {
            m_error!(
                ER::IndexOutOfRange,
                format!("invalid bucket {}", bucket_index)
            )
        })?;

        let deleted = {
            let _bucket_guard = bucket_lock.lock().await;
            let mut entries = self.read_bucket_entries(&dir.meta, bucket_index).await?;
            let old_len = entries.len();
            entries.retain(|(k, _)| k.as_slice() != key);
            if entries.len() == old_len {
                false
            } else {
                self.rewrite_bucket(&mut dir.meta, bucket_index, &entries)
                    .await?;
                true
            }
        };

        if !deleted {
            return Ok(false);
        }

        dir.meta.total_entries = dir.meta.total_entries.saturating_sub(1);
        self.write_meta_file(&self.meta_path(), &dir.meta).await?;
        Ok(true)
    }

    pub async fn config(&self) -> LinearHashConfig {
        let dir = self.directory.lock().await;
        LinearHashConfig {
            page_size: dir.meta.page_size,
            key_size: dir.meta.key_size,
            value_size: dir.meta.value_size,
            target_load_factor: dir.meta.target_load_factor,
            initial_buckets: 1 << dir.meta.level,
        }
    }

    #[cfg(test)]
    async fn bucket_count(&self) -> usize {
        let dir = self.directory.lock().await;
        dir.meta.bucket_page_ids.len()
    }

    async fn lock_bucket_for_key(&self, key: &[u8]) -> RS<(LinearHashMeta, usize, Arc<Mutex<()>>)> {
        let dir = self.directory.lock().await;
        validate_key_len(&dir.meta, key)?;
        let bucket_index = bucket_index_for_key(&dir.meta, key);
        let bucket_lock = dir.bucket_locks.get(bucket_index).cloned().ok_or_else(|| {
            m_error!(
                ER::IndexOutOfRange,
                format!("invalid bucket {}", bucket_index)
            )
        })?;
        Ok((dir.meta.clone(), bucket_index, bucket_lock))
    }

    async fn expand_if_needed_locked(&self, dir: &mut MutexGuard<'_, DirectoryState>) -> RS<()> {
        while current_load_factor(&dir.meta) > dir.meta.target_load_factor {
            self.expand_one_bucket_locked(dir).await?;
        }
        Ok(())
    }

    async fn expand_one_bucket_locked(&self, dir: &mut MutexGuard<'_, DirectoryState>) -> RS<()> {
        let base = 1usize << dir.meta.level;
        let split_bucket = dir.meta.split_pointer as usize;
        let new_bucket_index = base + split_bucket;

        let split_lock = dir
            .bucket_locks
            .get(split_bucket)
            .cloned()
            .ok_or_else(|| m_error!(ER::IndexOutOfRange, "split bucket lock not found"))?;
        let new_lock = Arc::new(Mutex::new(()));
        dir.bucket_locks.push(new_lock.clone());
        let _split_guard = split_lock.lock().await;
        let _new_guard = new_lock.lock().await;

        let new_page_id = alloc_page_id(&mut dir.meta);
        dir.meta.bucket_page_ids.push(new_page_id);

        self.write_bucket_page(
            &dir.meta,
            new_page_id,
            &BucketPage {
                next_page_id: None,
                entries: vec![],
            },
        )
        .await?;

        let old_entries = self.read_bucket_entries(&dir.meta, split_bucket).await?;
        let mut stay = vec![];
        let mut moved = vec![];
        let next_base = (base * 2) as u64;

        for (k, v) in old_entries {
            let h = fnv1a_hash(&k);
            let idx = (h % next_base) as usize;
            if idx == split_bucket {
                stay.push((k, v));
            } else {
                moved.push((k, v));
            }
        }

        self.rewrite_bucket(&mut dir.meta, split_bucket, &stay)
            .await?;
        self.rewrite_bucket(&mut dir.meta, new_bucket_index, &moved)
            .await?;

        dir.meta.split_pointer += 1;
        if dir.meta.split_pointer as usize == base {
            dir.meta.level += 1;
            dir.meta.split_pointer = 0;
        }
        Ok(())
    }

    async fn read_bucket_entries(
        &self,
        meta: &LinearHashMeta,
        bucket_index: usize,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        let chain = self.read_bucket_chain(meta, bucket_index).await?;
        let mut entries = Vec::with_capacity(chain.iter().map(|p| p.entries.len()).sum());
        for p in chain {
            entries.extend(p.entries.into_iter());
        }
        Ok(entries)
    }

    async fn read_bucket_chain(
        &self,
        meta: &LinearHashMeta,
        bucket_index: usize,
    ) -> RS<Vec<BucketPage>> {
        let mut pages = vec![];
        let mut page_id = primary_page_id(meta, bucket_index)?;
        loop {
            let page = self.read_bucket_page(meta, page_id).await?;
            let next = page.next_page_id;
            pages.push(page);
            if let Some(next_id) = next {
                page_id = next_id;
            } else {
                break;
            }
        }
        Ok(pages)
    }

    async fn read_bucket_chain_ids(
        &self,
        meta: &LinearHashMeta,
        bucket_index: usize,
    ) -> RS<Vec<u64>> {
        let mut ids = vec![];
        let mut page_id = primary_page_id(meta, bucket_index)?;
        loop {
            let page = self.read_bucket_page(meta, page_id).await?;
            let next = page.next_page_id;
            ids.push(page_id);
            if let Some(next_id) = next {
                page_id = next_id;
            } else {
                break;
            }
        }
        Ok(ids)
    }

    async fn rewrite_bucket(
        &self,
        meta: &mut LinearHashMeta,
        bucket_index: usize,
        entries: &[(Vec<u8>, Vec<u8>)],
    ) -> RS<()> {
        let mut chain_ids = self.read_bucket_chain_ids(meta, bucket_index).await?;
        if chain_ids.is_empty() {
            return Err(m_error!(ER::StorageErr, "bucket chain is empty"));
        }
        let primary_id = chain_ids[0];

        for released in chain_ids.drain(1..) {
            meta.free_page_ids.push(released);
        }

        let page_count = if entries.is_empty() {
            1
        } else {
            entries.len().div_ceil(meta.bucket_capacity)
        };

        let mut target_ids = vec![primary_id];
        while target_ids.len() < page_count {
            target_ids.push(alloc_page_id(meta));
        }

        for (i, page_id) in target_ids.iter().enumerate() {
            let begin = i * meta.bucket_capacity;
            let end = ((i + 1) * meta.bucket_capacity).min(entries.len());
            let chunk = if begin < end {
                &entries[begin..end]
            } else {
                &[]
            };
            let next = if i + 1 < target_ids.len() {
                Some(target_ids[i + 1])
            } else {
                None
            };
            let page = BucketPage {
                next_page_id: next,
                entries: chunk.to_vec(),
            };
            self.write_bucket_page(meta, *page_id, &page).await?;
        }
        Ok(())
    }

    async fn read_bucket_page(&self, meta: &LinearHashMeta, page_id: u64) -> RS<BucketPage> {
        read_page_data(
            &self.page_path(),
            page_id,
            meta.page_size,
            meta.key_size,
            meta.value_size,
            meta.bucket_capacity,
        )
        .await
    }

    async fn write_bucket_page(
        &self,
        meta: &LinearHashMeta,
        page_id: u64,
        page: &BucketPage,
    ) -> RS<()> {
        write_page_data(
            &self.page_path(),
            page_id,
            meta.page_size,
            meta.key_size,
            meta.value_size,
            page,
        )
        .await
    }

    async fn create_new(dir: PathBuf, config: LinearHashConfig) -> RS<Self> {
        let page_path = dir.join(PAGE_FILE_NAME);
        let meta_path = dir.join(META_FILE_NAME);
        let _file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&page_path)
            .await
            .map_err(|e| m_error!(ER::IOErr, "create page file error", e))?;

        let initial_buckets = config.initial_buckets.max(1);
        let level = initial_buckets.trailing_zeros();
        let bucket_capacity = config.bucket_capacity()?;
        let mut bucket_page_ids = Vec::with_capacity(initial_buckets as usize);

        for page_id in 0..initial_buckets as u64 {
            bucket_page_ids.push(page_id);
            write_page_data(
                &page_path,
                page_id,
                config.page_size,
                config.key_size,
                config.value_size,
                &BucketPage {
                    next_page_id: None,
                    entries: vec![],
                },
            )
            .await?;
        }

        let meta = LinearHashMeta {
            version: META_VERSION,
            page_size: config.page_size,
            key_size: config.key_size,
            value_size: config.value_size,
            bucket_capacity,
            target_load_factor: config.target_load_factor,
            level,
            split_pointer: 0,
            bucket_page_ids,
            next_page_id: initial_buckets as u64,
            free_page_ids: vec![],
            total_entries: 0,
        };

        let content = serde_json::to_vec_pretty(&meta)
            .map_err(|e| m_error!(ER::EncodeErr, "encode meta error", e))?;
        fs::write(&meta_path, content)
            .await
            .map_err(|e| m_error!(ER::IOErr, "write meta file error", e))?;

        let bucket_locks = (0..meta.bucket_page_ids.len())
            .map(|_| Arc::new(Mutex::new(())))
            .collect::<Vec<_>>();

        Ok(Self {
            dir,
            directory: Mutex::new(DirectoryState { meta, bucket_locks }),
        })
    }

    fn validate_meta_compat(meta: &LinearHashMeta, cfg: &LinearHashConfig) -> RS<()> {
        if meta.version != META_VERSION {
            return Err(m_error!(
                ER::ParseErr,
                format!("unsupported meta version {}", meta.version)
            ));
        }
        if meta.page_size != cfg.page_size
            || meta.key_size != cfg.key_size
            || meta.value_size != cfg.value_size
        {
            return Err(m_error!(
                ER::ParseErr,
                "config mismatch with persisted linear hash"
            ));
        }
        Ok(())
    }

    fn page_path(&self) -> PathBuf {
        self.dir.join(PAGE_FILE_NAME)
    }

    fn meta_path(&self) -> PathBuf {
        self.dir.join(META_FILE_NAME)
    }

    async fn write_meta_file(&self, path: &Path, meta: &LinearHashMeta) -> RS<()> {
        let content = serde_json::to_vec_pretty(meta)
            .map_err(|e| m_error!(ER::EncodeErr, "encode meta error", e))?;
        fs::write(path, content)
            .await
            .map_err(|e| m_error!(ER::IOErr, "write meta file error", e))?;
        Ok(())
    }

    async fn read_meta(path: &Path) -> RS<LinearHashMeta> {
        let buf = fs::read(path)
            .await
            .map_err(|e| m_error!(ER::IOErr, "read meta file error", e))?;
        serde_json::from_slice(&buf)
            .map_err(|e| m_error!(ER::DecodeErr, "decode meta file error", e))
    }
}

impl LinearHashConfig {
    fn validate(&self) -> RS<()> {
        if self.page_size <= PAGE_HEADER_SIZE {
            return Err(m_error!(ER::ParseErr, "page_size too small"));
        }
        if self.key_size == 0 || self.value_size == 0 {
            return Err(m_error!(ER::ParseErr, "key_size/value_size must be > 0"));
        }
        if !(0.1..=0.99).contains(&self.target_load_factor) {
            return Err(m_error!(
                ER::ParseErr,
                "target_load_factor must be in [0.1, 0.99]"
            ));
        }
        if !self.initial_buckets.is_power_of_two() {
            return Err(m_error!(
                ER::ParseErr,
                "initial_buckets must be power of two"
            ));
        }
        let capacity = self.bucket_capacity()?;
        if capacity == 0 {
            return Err(m_error!(ER::ParseErr, "page cannot hold any entry"));
        }
        Ok(())
    }

    fn bucket_capacity(&self) -> RS<usize> {
        let entry_size = self.key_size + self.value_size;
        if entry_size == 0 || self.page_size <= PAGE_HEADER_SIZE {
            return Err(m_error!(ER::ParseErr, "invalid page or entry size"));
        }
        Ok((self.page_size - PAGE_HEADER_SIZE) / entry_size)
    }
}

async fn read_page_data(
    page_path: &Path,
    page_id: u64,
    page_size: usize,
    key_size: usize,
    value_size: usize,
    bucket_capacity: usize,
) -> RS<BucketPage> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(page_path)
        .await
        .map_err(|e| m_error!(ER::IOErr, "open page file error", e))?;

    let mut buf = vec![0; page_size];
    seek_page(&mut file, page_id, page_size).await?;
    file.read_exact(&mut buf)
        .await
        .map_err(|e| m_error!(ER::IOErr, "read page error", e))?;

    let next = read_u64(&buf[0..8]);
    let count = read_u32(&buf[8..12]) as usize;
    if count > bucket_capacity {
        return Err(m_error!(
            ER::DecodeErr,
            format!("invalid entry count {} > {}", count, bucket_capacity)
        ));
    }

    let mut entries = Vec::with_capacity(count);
    let mut offset = PAGE_HEADER_SIZE;
    for _ in 0..count {
        let key = buf[offset..offset + key_size].to_vec();
        offset += key_size;
        let value = buf[offset..offset + value_size].to_vec();
        offset += value_size;
        entries.push((key, value));
    }

    Ok(BucketPage {
        next_page_id: if next == NONE_PAGE_ID {
            None
        } else {
            Some(next)
        },
        entries,
    })
}

async fn write_page_data(
    page_path: &Path,
    page_id: u64,
    page_size: usize,
    key_size: usize,
    value_size: usize,
    page: &BucketPage,
) -> RS<()> {
    let entry_size = key_size + value_size;
    let bucket_capacity = (page_size - PAGE_HEADER_SIZE) / entry_size;
    if page.entries.len() > bucket_capacity {
        return Err(m_error!(
            ER::StorageErr,
            format!(
                "entry count {} exceeds page capacity {}",
                page.entries.len(),
                bucket_capacity
            )
        ));
    }

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(page_path)
        .await
        .map_err(|e| m_error!(ER::IOErr, "open page file error", e))?;

    let mut buf = vec![0u8; page_size];
    write_u64(&mut buf[0..8], page.next_page_id.unwrap_or(NONE_PAGE_ID));
    write_u32(&mut buf[8..12], page.entries.len() as u32);

    let mut offset = PAGE_HEADER_SIZE;
    for (key, value) in &page.entries {
        if key.len() != key_size || value.len() != value_size {
            return Err(m_error!(
                ER::TypeErr,
                "entry key/value size does not match config"
            ));
        }
        buf[offset..offset + key_size].copy_from_slice(key);
        offset += key_size;
        buf[offset..offset + value_size].copy_from_slice(value);
        offset += value_size;
    }

    seek_page(&mut file, page_id, page_size).await?;
    file.write_all(&buf)
        .await
        .map_err(|e| m_error!(ER::IOErr, "write page error", e))?;
    file.flush()
        .await
        .map_err(|e| m_error!(ER::IOErr, "flush page file error", e))?;
    Ok(())
}

async fn seek_page(file: &mut tokio::fs::File, page_id: u64, page_size: usize) -> RS<()> {
    let offset = page_id
        .checked_mul(page_size as u64)
        .ok_or_else(|| m_error!(ER::IndexOutOfRange, "page seek overflow"))?;
    file.seek(SeekFrom::Start(offset))
        .await
        .map_err(|e| m_error!(ER::IOErr, "seek page error", e))?;
    Ok(())
}

fn validate_key_len(meta: &LinearHashMeta, key: &[u8]) -> RS<()> {
    if key.len() != meta.key_size {
        return Err(m_error!(
            ER::TypeErr,
            format!("invalid key len {}, expected {}", key.len(), meta.key_size)
        ));
    }
    Ok(())
}

fn validate_value_len(meta: &LinearHashMeta, value: &[u8]) -> RS<()> {
    if value.len() != meta.value_size {
        return Err(m_error!(
            ER::TypeErr,
            format!(
                "invalid value len {}, expected {}",
                value.len(),
                meta.value_size
            )
        ));
    }
    Ok(())
}

fn bucket_index_for_key(meta: &LinearHashMeta, key: &[u8]) -> usize {
    let hash = fnv1a_hash(key);
    let base = 1u64 << meta.level;
    let mut index = hash % base;
    if index < meta.split_pointer as u64 {
        index = hash % (base * 2);
    }
    index as usize
}

fn primary_page_id(meta: &LinearHashMeta, bucket_index: usize) -> RS<u64> {
    meta.bucket_page_ids
        .get(bucket_index)
        .copied()
        .ok_or_else(|| {
            m_error!(
                ER::IndexOutOfRange,
                format!("invalid bucket {}", bucket_index)
            )
        })
}

fn current_load_factor(meta: &LinearHashMeta) -> f64 {
    let buckets = meta.bucket_page_ids.len();
    if buckets == 0 || meta.bucket_capacity == 0 {
        return 0.0;
    }
    meta.total_entries as f64 / (buckets * meta.bucket_capacity) as f64
}

fn alloc_page_id(meta: &mut LinearHashMeta) -> u64 {
    if let Some(page_id) = meta.free_page_ids.pop() {
        return page_id;
    }
    let page_id = meta.next_page_id;
    meta.next_page_id += 1;
    page_id
}

fn fnv1a_hash(key: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &b in key {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn read_u64(buf: &[u8]) -> u64 {
    let mut arr = [0u8; 8];
    arr.copy_from_slice(buf);
    u64::from_le_bytes(arr)
}

fn read_u32(buf: &[u8]) -> u32 {
    let mut arr = [0u8; 4];
    arr.copy_from_slice(buf);
    u32::from_le_bytes(arr)
}

fn write_u64(buf: &mut [u8], v: u64) {
    buf.copy_from_slice(&v.to_le_bytes());
}

fn write_u32(buf: &mut [u8], v: u32) {
    buf.copy_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir() -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("linear_hash_test_{}", ts))
    }

    fn cfg() -> LinearHashConfig {
        LinearHashConfig {
            page_size: 128,
            key_size: 8,
            value_size: 16,
            target_load_factor: 0.7,
            initial_buckets: 2,
        }
    }

    fn key_u64(v: u64) -> [u8; 8] {
        v.to_le_bytes()
    }

    fn value_u64(v: u64) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[0..8].copy_from_slice(&v.to_le_bytes());
        out
    }

    #[tokio::test]
    async fn test_set_get_delete() {
        let dir = test_dir();
        let idx = LinearHash::open(&dir, cfg(), LinearHashOpenOptions::default())
            .await
            .unwrap();
        let k1 = key_u64(1);
        let v1 = value_u64(10);

        idx.set(&k1, &v1, None).await.unwrap();
        assert_eq!(idx.get(&k1, None).await.unwrap(), Some(v1.to_vec()));

        assert!(idx.delete(&k1, None).await.unwrap());
        assert_eq!(idx.get(&k1, None).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_persistent_reopen() {
        let dir = test_dir();
        let k1 = key_u64(42);
        let v1 = value_u64(99);
        {
            let idx = LinearHash::open(&dir, cfg(), LinearHashOpenOptions::default())
                .await
                .unwrap();
            idx.set(&k1, &v1, None).await.unwrap();
        }
        {
            let idx = LinearHash::open(
                &dir,
                cfg(),
                LinearHashOpenOptions {
                    create_if_missing: false,
                    create_new: false,
                },
            )
            .await
            .unwrap();
            assert_eq!(idx.get(&k1, None).await.unwrap(), Some(v1.to_vec()));
        }
    }

    #[tokio::test]
    async fn test_partial_expansion_happens() {
        let dir = test_dir();
        let idx = LinearHash::open(&dir, cfg(), LinearHashOpenOptions::default())
            .await
            .unwrap();
        let before = idx.bucket_count().await;

        for i in 0..50u64 {
            let k = key_u64(i);
            let v = value_u64(i * 10);
            idx.set(&k, &v, None).await.unwrap();
        }

        let after = idx.bucket_count().await;
        assert!(after > before);

        for i in 0..50u64 {
            let k = key_u64(i);
            let v = value_u64(i * 10);
            assert_eq!(idx.get(&k, None).await.unwrap(), Some(v.to_vec()));
        }
    }
}
