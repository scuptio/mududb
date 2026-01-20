use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{Atomic, AtomicBool, AtomicPtr, AtomicU64, AtomicU8, Ordering};
use tokio::sync::Mutex as AsMutex;
use tokio_condvar::Condvar as AsCondvar;
use mudu::common::endian;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use crate::storage::{constant, page_id};
use crate::storage::constant::PAGE_TAIL_SIZE;
use crate::storage::page_block::{Checksum, PageBlock};


const PAGE_UN_SETUP:u8 = 0;
const PAGE_LOADED:u8 = 1;
const PAGE_SWAPPED:u8 = 2;


enum State {
    PageUnSetup,
    PageLoaded,
    PageSwapped,
}

pub struct CtrlInner {
    frame_index: u64,
    is_dirty:AtomicBool,
    is_fixed:AtomicBool,
    is_swaping:AtomicBool,
    used_count:AtomicU64,
    state:AsMutex<State>,
    condvar: AsCondvar
}

#[derive(Clone)]
pub struct FrameCtrl {
    inner:Arc<CtrlInner>,
}


impl CtrlInner {
    fn new(frame_index:u64) -> Self {
        Self {
            frame_index,
            is_fixed: AtomicBool::new(false),
            is_dirty: AtomicBool::new(false),
            is_swaping:AtomicBool::new(false),
            used_count:AtomicU64::new(0),
            state: AsMutex::new(State::PageUnSetup),
            condvar: Default::default(),
        }
    }
}

fn write_header(page:&mut PageBlock, page_id:u64, lsn:u64) {
    page.set_page_id(page_id);
    page.set_lsn(lsn);
}

impl FrameCtrl {
    pub fn new_empty(frame_index:u64) -> Self {
        let inner = Arc::new(CtrlInner::new(frame_index));
        Self {
            inner:inner.clone(),
        }
    }

    pub fn notify(&self) {
        self.inner.condvar.notify_all();
    }

    pub fn set_fixed(&mut self, fixed:bool)  {
        self.inner.is_fixed.store(fixed, Ordering::SeqCst);
    }

    pub fn set_dirty(&mut self, dirty:bool) {
        self.inner.is_dirty.store(dirty, Ordering::SeqCst);
    }

    /// Calculate checksum for page data integrity
    fn calculate_checksum(page:&PageBlock) -> Checksum {
        page.block()[0.. page.block().len() - PAGE_TAIL_SIZE]
            .iter()
            .fold(0u64,
                  |acc, &byte| acc.wrapping_add(byte as u64)
            )
    }

    /// Write data to the page at specific offset
    fn write_data(&mut self, offset: usize, data: &[u8], page:&mut PageBlock) -> RS<()> {
        if offset + data.len() > page.block().len() - PAGE_TAIL_SIZE {
            return Err(m_error!(
                EC::StorageErr,
                "Data exceeds page boundary"
            ))?;
        }
        page.block_mut()[offset..offset + data.len()].copy_from_slice(data);
        let checksum = Self::calculate_checksum(page);
        page.set_checksum(checksum);
        self.set_dirty(true);
        Ok(())
    }

    /// Read data from the page
    pub fn read_data(&self, offset: usize, length: usize, page:&PageBlock) -> RS<Vec<u8>> {
        if offset + length > page.block().len() - PAGE_TAIL_SIZE {
            return Err(m_error!(
                EC::StorageErr,
                "Read beyond page boundary"
            ))?;
        }
        Ok(page.block()[offset..offset + length].to_vec())
    }

    pub fn use_frame(&mut self) -> bool {
        self.inner.used_count.fetch_add(1, Ordering::SeqCst);
        if self.inner.is_swaping.load(Ordering::SeqCst) {
            self.inner.used_count.fetch_sub(1, Ordering::SeqCst);
            false
        } else {
            true
        }
    }

    pub fn unuse_frame(&mut self) {
        let n = self.inner.used_count.fetch_sub(1, Ordering::SeqCst);
        if n == 1 && self.inner.is_swaping.load(Ordering::SeqCst) {
            self.inner.condvar.notify_all();
        }
    }

    pub async fn swap_out(&self) -> RS<bool> {
        let is_fixed = self.inner.is_fixed.load(Ordering::SeqCst);
        if is_fixed {
            return Err(m_error!(EC::StorageErr, "cannot sawp out a fixed page"));
        }
        self.inner.is_swaping.store(true, Ordering::SeqCst);
        loop {
            let mut guard = self.inner.state.lock().await;
            if self.inner.used_count.load(Ordering::SeqCst) == 0 {
                *guard =  State::PageSwapped;
                break;
            }
            self.inner.condvar.wait(guard).await;
        }
        let is_dirty = self.inner.is_dirty.load(Ordering::SeqCst);
        Ok(is_dirty)
    }
    
    pub fn reset(&mut self) {
        self.inner.is_dirty.store(false, Ordering::SeqCst);
        self.inner.is_fixed.store(false, Ordering::SeqCst);
        self.inner.is_swaping.store(false, Ordering::SeqCst);
        self.inner.used_count.store(0, Ordering::SeqCst);
    }
}