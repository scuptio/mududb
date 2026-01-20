use crate::storage::page_block::PageBlock;
use crate::storage::page_index::PageIndex;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use scc::HashMap;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex as AsMutex;


pub struct DiskIO {
    path: String,
    page_size: u64,
    files: HashMap<u64, DiskFile>
}

#[derive(Clone)]
struct DiskFile {
    file: Arc<AsMutex<File>>,
}

impl DiskIO {
    pub fn new(path: String, page_size: u64) -> RS<Self> {
        Ok(Self {
            path,
            page_size,
            files: HashMap::new(),
        })
    }

    pub async fn write_page(&self, page_index:PageIndex, page_block:PageBlock) -> RS<()> {
        let file = self.file_get_or_create(page_index.file_id)?;
        let offset = page_index.page_id * self.page_size;
        file.write_page(offset, page_block.block()).await?;
        Ok(())
    }

    pub async fn read_page(&self, page_index:PageIndex, page_block: &mut PageBlock) -> RS<()> {
        let file = self.file_get_or_create(page_index.file_id)?;
        let offset = page_index.page_id * self.page_size;
        file.read_page(offset, page_block.block_mut()).await?;
        Ok(())
    }

    fn file_get_or_create(&self, file_id: u64) -> RS<DiskFile> {
        let opt = self.files.get_sync(&file_id);
        let file = match opt {
            Some(file) => file.clone(),
            None => {
                let path_buf = PathBuf::from(&self.path).join(file_id.to_string());
                let file = DiskFile::new(&path_buf)?;
                self.files.insert_sync(file_id, file.clone());
                file
            }
        };
        Ok(file)
    }
}

impl DiskFile {
    fn new<P: AsRef<Path>>(path: P) -> RS<Self> {
        let file = std::fs::File::open(path.as_ref())
            .map_err(|e| m_error!(EC::IOErr, "open file error", e))?;
        let file = File::from_std(file);
        Ok(Self {
            file: Arc::new(AsMutex::new(file)),
        })
    }

    async fn write_page(&self, offset: u64, block: &[u8]) -> RS<()> {
        let mut file = self.file.lock().await;
        file.seek(SeekFrom::Start(offset)).await
            .map_err(|e| m_error!(EC::IOErr, "seek file error", e))?;
        file.write_all(&block).await
            .map_err(|e| m_error!(EC::IOErr, "write block error", e))?;
        Ok(())
    }

    async fn read_page(&self, offset: u64, block: &mut [u8]) -> RS<()> {
        let mut file = self.file.lock().await;
        file.seek(SeekFrom::Start(offset)).await
            .map_err(|e| m_error!(EC::IOErr, "seek file error", e))?;
        file.read_exact(block).await
            .map_err(|e| m_error!(EC::IOErr, "read block error", e))?;
        Ok(())
    }
}

