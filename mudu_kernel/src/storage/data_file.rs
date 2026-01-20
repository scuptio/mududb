use crate::storage::disk_io::DiskIO;
use crate::storage::extent::Extent;
use crate::storage::log_entry::LogEntry;
use crate::storage::page_block::{PageBlock, PageUpdate};
use crate::storage::page_index::PageIndex;
use crate::storage::storage_cfg::StorageCfg;
use crate::storage::storage_context::StorageContext;
use mcslock::raw::Mutex;
use mcslock::relax::Spin;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use roaring::RoaringBitmap;
use scc::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static NEXT_FILE_ID:AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct DataFile {
    inner:Arc<DataFileInner>
}

struct DataFileInner {
    extent_pages : u64,
    page_size : u64,
    file_id: u64,
    path: String,
    disk_io: DiskIO,
    total_allocated: AtomicU64,
    roaring:Mutex<RoaringBitmap, Spin>,
    allocated_extents: HashMap<u64, Extent>,
}

impl DataFile {
    pub fn open(cfg: &StorageCfg) -> RS<Self> {
        Ok(Self {
            inner: Arc::new(DataFileInner::open(cfg)?)
        })
    }

    pub fn current_file_id() -> u64 {
        _current_file_id()
    }

    pub fn next_file_id() -> u64 {
        _next_file_id()
    }

    pub fn file_create_extent(&self, table_space:u64, ctx:&StorageContext) -> RS<Extent> {
        self.inner.create_extent(table_space, ctx)
    }

    pub async fn write_block(&self, block:PageBlock) -> RS<()> {
        self.inner.write_block(block).await?;
        Ok(())
    }
}


fn _current_file_id() -> u64 {
    NEXT_FILE_ID.load(Ordering::SeqCst)
}

fn _next_file_id() -> u64 {
    NEXT_FILE_ID.fetch_add(1, Ordering::SeqCst)
}

impl DataFileInner {
    fn open(cfg: &StorageCfg) -> RS<Self> {
        let (file_id, path_buf) = loop {
            let file_id = _next_file_id();
            let file_path = PathBuf::from(&cfg.path).join(file_id.to_string());
            if !file_path.exists() {
                break (file_id, file_path);
            }
        };

        Ok(Self::new(
            cfg,
            file_id,
        )?)
    }

    fn new(
        cfg:&StorageCfg,
        file_id: u64,
    ) -> RS<Self> {
        Ok(Self {
            extent_pages: cfg.extent_pages,
            page_size: cfg.page_size,
            file_id,
            path: cfg.path.clone(),
            disk_io: DiskIO::new(cfg.path.clone(), cfg.page_size)?,
            total_allocated: AtomicU64::new(0),
            roaring: Default::default(),
            allocated_extents: Default::default(),
        })
    }


    fn create_extent(&self, table_space_id:u64, context:&StorageContext) -> RS<Extent> {
        let opt = self.get_free_extent();
        if let Some(n) = opt {
            let extent_id = self.extent_page_id(n);
            let extent = context.extent_get(&PageIndex::new(self.file_id, extent_id))
                .map_or_else(
                    || { Err(m_error!(EC::StorageErr, "cannot get extent")) },
                    |e| { Ok(e) })?;
            Ok(extent)
        } else {
            let index = self.total_allocated.load(Ordering::SeqCst);
            let extent_id = self.extent_page_id(index);
            let extent = Extent::new(
                self.file_id,
                table_space_id,
                extent_id, extent_id + 1, self.extent_pages);
            let mut block = PageBlock::new_empty(self.page_size);
            block.set_page_id(extent_id);
            block.set_lsn(0);

            let page_id = extent_id;
            let mut log_entry = LogEntry::new(self.file_id, extent_id * self.page_size);
            block.set_lsn(log_entry.lsn());
            block.reset_checksum();
            log_entry.add_update_delta(PageUpdate::new(page_id, 0, block.block().to_vec()));
            context.append_log_entry(log_entry);

            self.total_allocated.fetch_add(1, Ordering::SeqCst);
            Ok(extent)
        }
    }


    fn extent_page_id(&self, index:u64) -> u64 {
        index * self.extent_pages
    }

    async fn write_block(& self, block:PageBlock) -> RS<()> {
        let page_id = block.get_page_id();
        self.disk_io.write_page(PageIndex::new(self.file_id, page_id), block).await
    }

    fn get_free_extent(&self) -> Option<u64> {
        self.roaring.lock_then(|roaring| {
            let opt = roaring.iter().next();
            if let Some(n) = opt {
                roaring.remove(n);
                Some(n as u64)
            } else {
                None
            }
        })
    }

    fn push_free_extent(&self, extent:u64) {
        self.roaring.lock_then(|roaring| {
            roaring.insert(extent as u32);
        })
    }
}
