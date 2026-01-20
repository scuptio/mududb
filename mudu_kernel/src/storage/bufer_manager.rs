use crate::storage::disk_io::DiskIO;
use crate::storage::frame::Frame;
use crate::storage::page_block::PageBlock;
use crate::storage::page_index::PageIndex;
use crate::storage::storage_cfg::StorageCfg;
use mcslock::raw::Mutex;
use mcslock::relax::Spin;
use mudu::common::result::RS;
use rand::seq::IteratorRandom;
use rand::rng as thread_rng;
use scc::HashMap;
use std::sync::Arc;

pub struct BufferManager {
    inner: Arc<BufferManagerInner>,
}
pub struct BufferManagerInner {
    page_size:u64,
    page_cache:HashMap<PageIndex, Frame>,
    frame_array:Vec<Frame>,
    free_list:Mutex<FreeList, Spin>,
    disk_io:DiskIO,
}


impl BufferManager {
    pub fn new(cfg: &StorageCfg) -> RS<Self> {
        Ok(Self {
            inner: Arc::new(BufferManagerInner::new(cfg)?)
        })
    }

    pub async fn get_page(&self, page_index:&PageIndex) -> RS<Frame> {
        self.inner.get_page(page_index).await
    }
}

struct FreeList {
    vec:Vec<u64>,
}

impl FreeList {
    fn new(vec:Vec<u64>) -> FreeList {
        Self {vec}
    }

    fn get_a_free_frame_index(&mut self) -> Option<u64> {
        let mut rng = thread_rng();
        if let Some(index) = (0..self.vec.len()).choose(&mut rng) {
            // Use swap_remove(index) to remove the element efficiently (O(1))
            // The last element is swapped into the removed element's place
            let removed_element = self.vec.swap_remove(index);
            Some(removed_element)
        } else {
            None
        }
    }

    fn add_a_free_frame_index(&mut self, fram_index:u64) {
        let result = self.vec.binary_search(&fram_index);
        match result {
            Ok(_) => {}
            Err(index) => {
                self.vec.insert(index, fram_index);
            }
        }
    }

    fn remove_free_frame_index(&mut self, fram_index:u64) {
        let result = self.vec.binary_search(&fram_index);
        match result {
            Ok(index) => {
                self.vec.remove(index);
            }
            Err(_) => {
            }
        }
    }
}

impl BufferManagerInner {
    fn new(cfg: &StorageCfg) -> RS<Self> {
        let mut buffer = Vec::with_capacity(cfg.buffer_num_pages as usize);
        let mut free_list = Vec::with_capacity(cfg.buffer_num_pages as usize);
        for frame_index in 0..cfg.buffer_num_pages {
            let frame = Frame::new_empty(frame_index, cfg.page_size);
            free_list.push(frame_index);
        }

        Ok(Self {
            page_size: cfg.page_size,
            page_cache: Default::default(),
            frame_array: buffer,
            free_list: Mutex::new(FreeList::new(free_list)),
            disk_io: DiskIO::new(cfg.path.clone(), cfg.page_size)?
        })
    }


    fn get_a_free_frame(&self) -> Option<Frame> {
        let mut rng = thread_rng();
        let opt = self.free_list.lock_then(|fl| {
            fl.get_a_free_frame_index()
        });
        match opt {
            Some(index) => { self.frame_array.get(index as usize).cloned() },
            None => { None }
        }
    }

    async fn locate_a_frame(&self) -> RS<Frame> {
        let frame = self.get_a_free_frame().unwrap();
        let page_index = frame.page_index().clone();
        // remove from the cache if it exists in cache
        self.page_cache.remove_sync(&page_index);
        let mut page_block = PageBlock::new_empty(self.page_size);
        frame.copy_page(&mut page_block)?;
        let is_dirty = frame.swap_out().await?;
        if is_dirty {
            self.disk_io.write_page(page_index, page_block).await?;
            frame.reset();
        }
        Ok(frame)
    }

    pub async fn get_page(&self, page_index: &PageIndex) -> RS<Frame> {
        let opt = self.page_cache.get_sync(page_index);
        match opt {
            Some(frame) => {
                Ok(frame.clone())
            },
            None => {
                let frame = self.locate_a_frame().await?;
                let mut page_block = PageBlock::new_empty(self.page_size);
                self.disk_io.read_page(page_index.clone(), &mut page_block).await?;
                frame.swap_page(&mut page_block);
                Ok(frame)
            }
        }
    }
}