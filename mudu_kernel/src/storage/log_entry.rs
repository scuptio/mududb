use std::sync::atomic::{AtomicU64, Ordering};
use crate::storage::page_block::PageUpdate;

static LSN:AtomicU64 = AtomicU64::new(0);


pub fn lsn_init(n:u64) {
    LSN.store(n, Ordering::SeqCst)
}

pub fn next_lsn() -> u64 {
    LSN.fetch_add(1, Ordering::SeqCst)
}

pub struct LogEntry {
    lsn:u64,
    file_id:u64,
    page_id:u64,
    update:Vec<PageUpdate>,
}



impl LogEntry {
    pub fn new(file_id:u64, page_id:u64) -> LogEntry {
        Self {
            lsn: next_lsn(),
            file_id,
            page_id,
            update:vec![],
        }
    }

    pub fn add_update_delta(&mut self, update: PageUpdate) {
        self.update.push(update);
    }

    pub fn lsn(&self) -> u64 {
        self.lsn
    }

    pub fn file_id(&self) -> u64 {
        self.file_id
    }

    pub fn page_id(&self) -> u64 {
        self.page_id
    }

    pub fn update(&self) -> &Vec<PageUpdate> {
        &self.update
    }
}