use crate::storage::bufer_manager::BufferManager;
use crate::storage::data_file::DataFile;
use crate::storage::extent::Extent;
use crate::storage::log_entry::LogEntry;
use crate::storage::page_index::PageIndex;
use crate::storage::space_manager::SpaceManager;
use crate::storage::storage_cfg::StorageCfg;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use scc::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct StorageContext {
    inner: Arc<ContextInner>,
}

impl StorageContext {
    pub fn new(cfg: &StorageCfg) -> RS<Self> {
        Ok(Self {
            inner: Arc::new(ContextInner::new(cfg)?)
        })
    }

    pub fn extent_insert(&self, page_index: PageIndex, extent: Extent) -> RS<()> {
        self.inner.extent_insert(page_index, extent)
    }

    pub fn extent_get(&self, page_index: &PageIndex) -> Option<Extent> {
        self.inner.extent_get(page_index)
    }

    pub fn file_get(&self, data_file_id: u64) -> Option<DataFile> {
        self.inner.file_get(data_file_id)
    }

    pub fn file_insert(&self, data_file_id: u64, data_file: DataFile) -> RS<()> {
        self.inner.file_insert(data_file_id, data_file)
    }
    pub fn append_log_entry(&self, _log:LogEntry) {
        todo!()
    }

    pub fn buffer_manager(&self) -> &BufferManager {
        self.inner.buffer_manager()
    }

    pub fn cfg(&self) -> &StorageCfg {
        &self.inner.cfg
    }
}

impl ContextInner {
    pub fn new(cfg: &StorageCfg) -> RS<Self> {
        let buffer_manager = BufferManager::new(cfg)?;
        let space_manager = SpaceManager::new(cfg)?;
        Ok(Self {
            cfg: cfg.clone(),
            extent: Default::default(),
            data_files: Default::default(),
            buffer_manager,
            space_manager,
        })
    }

    pub fn extent_insert(&self, page_index: PageIndex, extent: Extent) -> RS<()> {
        self.extent.insert_sync(page_index, extent)
            .map_err(|_|{
                m_error!(EC::StorageErr, "exsiting such extent")
            })
    }

    pub fn extent_get(&self, page_index: &PageIndex) -> Option<Extent> {
        self.extent.get_sync(page_index).map(|x| x.clone())
    }


    fn file_insert(&self, data_file_id: u64, data_file: DataFile) -> RS<()> {
        self.data_files.insert_sync(data_file_id, data_file);
        Ok(())
    }

    fn file_get(&self, data_file_id: u64) -> Option<DataFile> {
        self.data_files.get_sync(&data_file_id).map(|x| x.clone())
    }

    pub fn buffer_manager(&self) -> &BufferManager {
        &self.buffer_manager
    }
}

pub struct ContextInner {
    cfg: StorageCfg,
    extent: HashMap<PageIndex, Extent>,
    data_files:HashMap<u64, DataFile>,
    buffer_manager: BufferManager,
    space_manager: SpaceManager,
}