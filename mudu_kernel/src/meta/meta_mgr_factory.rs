use crate::contract::meta_mgr::MetaMgr;
use crate::meta::meta_mgr::MetaMgrImpl;
use mudu::common::result::RS;
use std::path::PathBuf;
use std::sync::Arc;

pub struct MetaMgrFactory {}

impl MetaMgrFactory {
    pub fn create(path: String) -> RS<Arc<dyn MetaMgr>> {
        let mut path = PathBuf::from(path);
        path.push("meta");
        let meta_mgr = MetaMgrImpl::new(path)?;
        Ok(Arc::new(meta_mgr))
    }
}
