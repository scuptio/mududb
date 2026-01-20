use crate::storage::data_file::DataFile;
use crate::storage::extent::Extent;
use crate::storage::page_index::PageIndex;
use crate::storage::storage_context::StorageContext;
use mcslock::raw::Mutex;
use mcslock::relax::Spin;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::json::to_json_str;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct TableSpace {
    inner:Arc<TableSpaceInner>
}

#[derive(Serialize, Deserialize)]
struct TableExtent {
    table_id:u64,
    extent:Vec<PageIndex>,
}

struct TableSpaceInner {
    table_id: u64,
    name: String,
    path: String,
    extent_index: Mutex<Vec<PageIndex>, Spin>,
}


impl TableSpace {
    pub fn new(id:u64, path:String) -> RS<TableSpace> {
        Ok(Self {
            inner:Arc::new(TableSpaceInner::new(id, path)?)
        })
    }

    pub fn allocate_page(&self, ctx:&StorageContext) -> RS<PageIndex> {
        self.inner.allocate_page(ctx)
    }
}

impl TableSpaceInner {
    fn flush(&self) -> RS<()> {
        let table_extent = self.to_table_extent();
        let path_buf = PathBuf::from(&self.path).join(self.table_id.to_string());
        let json = to_json_str(&table_extent)?;
        fs::write(path_buf, json).map_err(|e| m_error!(EC::IOErr, "save table space extent error"))?;
        Ok(())
    }

    fn new(id: u64, path:String) -> RS<TableSpaceInner> {
        let path_buf = PathBuf::from(&path);
        if path_buf.exists() {
            if !path_buf.is_dir() {
               return  Err(m_error!(EC::IOErr, format!("{:?} is not a directory", path_buf)));
            }
        } else {
            fs::create_dir_all(&path_buf).map_err(|e|{
                m_error!(EC::IOErr, "create dir error")
            })?;
        }
        Ok(Self {
            table_id: id,
            name: "".to_string(),
            extent_index: Mutex::new(vec![]),
            path,
        })
    }

    fn table_space_create_extent(&self, ctx:&StorageContext) -> RS<Extent> {
        let id = DataFile::current_file_id();
        let opt_data_file = ctx.file_get(DataFile::current_file_id());
        let data_file = match opt_data_file {
            Some(file) => { file }
            None => {
                let file = DataFile::open(ctx.cfg())?;
                file
            }
        };
        let extent = data_file.file_create_extent(self.table_id, ctx)?;
        Ok(extent)
    }

    fn allocate_page(& self, ctx:&StorageContext) -> RS<PageIndex> {
        let opt = match self.extent_index_last() {
            Some(index) => {
                ctx.extent_get(&index)
            }
            None => { None }
        };
        let extent= if let Some(extent) = opt {
            extent
        } else {
            let extent = self.table_space_create_extent(ctx)?;
            let page_index = PageIndex::new(extent.file_id(), extent.extent_id());
            self.extent_index_push(page_index);
            extent
        };
        let opt_page = extent.extent_allocate_page();
        let page_index = if let Some(page_id) = opt_page {
            PageIndex::new(extent.file_id(), page_id)
        } else {
            let extent = self.table_space_create_extent(ctx)?;
            let opt = extent.extent_allocate_page();
            if let Some(page_id) = opt {
                PageIndex::new(extent.file_id(), page_id)
            } else {
                return Err(m_error!(EC::StorageErr, "cannot allocate page"))
            }
        };
        Ok(page_index)
    }

    fn to_table_extent(&self) -> TableExtent {
        TableExtent {
            table_id:self.table_id,
            extent:self.copy_extent_index(),
        }
    }

    fn copy_extent_index(&self) -> Vec<PageIndex> {
        self.extent_index.lock_then(|i| i.clone())
    }

    fn extent_index_push(&self, index:PageIndex) {
        self.extent_index.lock_then(|i| i.push(index))
    }

    fn extent_index_last(&self) -> Option<PageIndex> {
        self.extent_index.lock_then(|i| i.last().cloned() )
    }
}