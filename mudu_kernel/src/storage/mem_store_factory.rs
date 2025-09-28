use crate::contract::mem_store::MemStore;
use crate::storage::mem_store::MemStoreImpl;
use mudu::common::result::RS;
use std::sync::Arc;

pub struct MemStoreFactory {}

impl MemStoreFactory {
    pub fn create(_path: String) -> RS<Arc<dyn MemStore>> {
        Ok(Arc::new(MemStoreImpl::new()))
    }
}
