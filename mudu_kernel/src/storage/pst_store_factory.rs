use crate::contract::s_task::SyncTask;
use crate::storage::pst_op_ch::PstOpCh;
use crate::storage::pst_store_impl::{PstOpChImpl, PstStoreImpl};
use crate::sync::unique_inner::UniqueInner;
use mudu::common::result::RS;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::Arc;

pub struct PstStoreFactory {}

impl PstStoreFactory {
    pub fn create(db_path: String) -> RS<(SyncTask, Arc<dyn PstOpCh>)> {
        let mut path = PathBuf::from(db_path);
        path.push("db");
        if !path.exists() {
            fs::create_dir_all(&path).unwrap();
        }
        path.push("kv.db");
        let path = path.to_str().unwrap().to_string();
        let (s, r) = channel();
        let store = PstStoreImpl::new(path, r)?;
        let task = Arc::new(UniqueInner::new(store));
        let ch = PstOpChImpl::new(s);
        Ok((task, Arc::new(ch)))
    }
}
