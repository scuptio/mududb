use super::{invalid, RS};
use async_trait::async_trait;
use mudu_sys::{contract::async_file::AsyncFile, sync::async_::AMutex};
use std::{collections::HashMap, sync::Arc};

/// Async backing for fixed-size index pages. Page zero is reserved.
/// Implementations must return the last successfully written complete image.
#[async_trait]
pub trait PageStore: Send + Sync {
    /// Reads one encoded page.
    async fn read(&self, page_id: u64) -> RS<Vec<u8>>;
    /// Writes one complete encoded page.
    async fn write(&self, page_id: u64, bytes: &[u8]) -> RS<()>;
    /// Releases unreachable pages, after all readers have left their epoch.
    /// File stores may keep their extents for subsequent page-ID reuse.
    async fn reclaim(&self, _page_ids: &[u64]) -> RS<()> {
        Ok(())
    }
    /// Makes prior writes durable.
    async fn flush(&self) -> RS<()>;
}

/// Non-evicting backing storage for tests and in-memory trees.
#[derive(Default)]
pub struct MemoryPageStore {
    pages: AMutex<HashMap<u64, Vec<u8>>>,
}

#[async_trait]
impl PageStore for MemoryPageStore {
    async fn read(&self, page_id: u64) -> RS<Vec<u8>> {
        self.pages
            .lock()
            .await
            .get(&page_id)
            .cloned()
            .ok_or_else(|| invalid("missing index page"))
    }
    async fn write(&self, page_id: u64, bytes: &[u8]) -> RS<()> {
        self.pages.lock().await.insert(page_id, bytes.to_vec());
        Ok(())
    }
    async fn reclaim(&self, page_ids: &[u64]) -> RS<()> {
        let mut pages = self.pages.lock().await;
        for pid in page_ids {
            pages.remove(pid);
        }
        Ok(())
    }
    async fn flush(&self) -> RS<()> {
        Ok(())
    }
}

/// Fixed-offset async file backing. Open on a dedicated rebuildable cache file.
/// A running cache file is not, by itself, a consistent restart checkpoint.
pub struct FilePageStore {
    file: Arc<dyn AsyncFile>,
    page_size: usize,
    // Logical page numbers are biased down before offset computation, so a
    // consumer whose on-disk numbering starts at zero can sit on a manager
    // that reserves logical page zero.
    bias: u64,
    lanes: Vec<super::OrderedFile>,
}

impl FilePageStore {
    /// Uses a caller-owned async file and the tree's fixed page size.
    pub fn new(file: Arc<dyn AsyncFile>, page_size: usize) -> RS<Self> {
        Self::with_page_bias(file, page_size, 0)
    }
    /// Uses a caller-owned async file; backing file page zero maps to logical
    /// page `bias`.
    pub fn with_page_bias(file: Arc<dyn AsyncFile>, page_size: usize, bias: u64) -> RS<Self> {
        if page_size == 0 {
            return Err(invalid("page size must be nonzero"));
        }
        Ok(Self {
            lanes: (0..16)
                .map(|_| super::OrderedFile::new(file.clone()))
                .collect(),
            file,
            page_size,
            bias,
        })
    }
    fn offset(&self, pid: u64) -> RS<u64> {
        pid.checked_sub(self.bias)
            .and_then(|pid| pid.checked_mul(self.page_size as u64))
            .ok_or_else(|| invalid("index file offset overflow"))
    }
}

#[async_trait]
impl PageStore for FilePageStore {
    async fn read(&self, page_id: u64) -> RS<Vec<u8>> {
        self.lanes[page_id as usize % self.lanes.len()]
            .read_exact_at(self.offset(page_id)?, self.page_size)
            .await
    }
    async fn write(&self, page_id: u64, bytes: &[u8]) -> RS<()> {
        if bytes.len() != self.page_size {
            return Err(invalid("wrong index page size"));
        }
        self.lanes[page_id as usize % self.lanes.len()]
            .write_all_at(self.offset(page_id)?, bytes)
            .await
    }
    async fn flush(&self) -> RS<()> {
        for lane in &self.lanes {
            lane.drain().await?;
        }
        self.file.fsync().await
    }
}
