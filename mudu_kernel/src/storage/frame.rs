use std::sync::Arc;
use mcslock::raw::Mutex;
use mcslock::relax::Spin;
use tokio::sync::RwLock;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use crate::storage::frame_ctrl::FrameCtrl;
use crate::storage::page_block::PageBlock;
use crate::storage::page_id;
use crate::storage::page_index::PageIndex;

#[derive(Clone)]
pub struct Frame {
    inner: Arc<FrameInner>,
}

pub struct FrameInner {
    page_index:PageIndex,
    page:Mutex<CtrlBlock, Spin>,
}

pub struct CtrlBlock {
    frame_ctrl: FrameCtrl,
    page_block: PageBlock,
}

impl FrameInner {
    fn new_empty(frame_id:u64, page_size:u64) -> Self {
        Self {
            page_index: PageIndex::invalid_index(),
            page:Mutex::new(
                CtrlBlock::new_empty(frame_id, page_size)
            )
        }
    }
}

impl CtrlBlock {
    
    fn new_empty(frame_id:u64, page_size:u64) -> Self {
        Self {
            frame_ctrl:FrameCtrl::new_empty(frame_id),
            page_block: PageBlock::new_empty(page_size)
        }
    }
    
    fn new(frame_ctrl: FrameCtrl, page_block: PageBlock) -> CtrlBlock {
        Self { frame_ctrl, page_block }
    }

    fn swap_page(&mut self, block:&mut PageBlock) -> RS<()> {
        self.page_block.swap(block);
        Ok(())
    }

    fn read_block(&mut self, block:&mut PageBlock) {
        self.page_block.copy_block(block);
    }
}

impl Frame {
    pub fn new_empty(frame_id:u64, page_size:u64) -> Frame {
        Self {
            inner: Arc::new(FrameInner::new_empty(frame_id, page_size)),
        }
    }

    pub fn from_page(frame_ctrl: FrameCtrl, file_id:u64, page_block:PageBlock) -> Frame {
        let page_id = page_block.get_page_id();
        let ctrl_block = CtrlBlock::new(frame_ctrl, page_block);
        Self {
            inner: Arc::new(
                FrameInner{
                    page_index: PageIndex::new(file_id, page_id),
                    page:Mutex::new(ctrl_block)
                })
        }
    }

    pub fn page_index(&self) -> &PageIndex {
        &self.inner.page_index
    }

    pub fn set_fixed(&self, fixed:bool) -> RS<()> {
        self.inner.page.lock_then(|cb| {
            cb.frame_ctrl.set_fixed(fixed); Ok(()) }
        )?;
        Ok(())
    }

    pub fn swap_page(&self, block:&mut PageBlock) -> RS<()> {
        self.inner.page.lock_then(move |cb|{
            cb.swap_page(block)?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn copy_page(&self, block:&mut PageBlock) -> RS<()> {
        self.inner.page.lock_then(|cb|{
            cb.read_block(block);
            Ok(())
        })
    }

    // swap out a page, return true if its dirty flag was set.
    pub async fn swap_out(&self) -> RS<bool> {
        let ctrl = self.inner.page.lock_then(|cb|{
            cb.frame_ctrl.clone()
        });
        let is_dirty = ctrl.swap_out().await?;
        Ok(is_dirty)
    }

    pub fn reset(&self) -> RS<()> {
        self.inner.page.lock_then(|cb|{
            cb.frame_ctrl.reset();
        });
        Ok(())
    }
}

