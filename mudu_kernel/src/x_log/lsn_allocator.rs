use crate::contract::lsn::LSN;
use mudu::common::result::RS;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

#[derive(Clone)]
pub struct LSNAllocator {
    lsn: Arc<AtomicU64>,
}

impl LSNAllocator {
    pub fn new() -> Self {
        Self {
            lsn: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn next(&self) -> LSN {
        let n = self.lsn.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        n + 1
    }

    pub fn alloc_max(&self) -> LSN {
        self.lsn.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn recovery(&self, lsn: LSN) -> RS<()> {
        self.lsn.store(lsn, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}
