use crate::common::std_file::StdAsyncFile;
use crate::contract::file_options::FileOptions;
pub use crate::io::sys_file::SysFile;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::path::Path;
use std::sync::Arc;

pub async fn open(path: impl AsRef<Path>) -> RS<SysFile> {
    let file = crate::imp::env::Sys::fs()
        .open(path.as_ref(), FileOptions::read_write_create())
        .await?;
    Ok(SysFile::new(file))
}

pub async fn open_with_options(path: impl AsRef<Path>, options: FileOptions) -> RS<SysFile> {
    let file = crate::imp::env::Sys::fs()
        .open(path.as_ref(), options)
        .await?;
    Ok(SysFile::new(file))
}

pub async fn open_raw(path: impl AsRef<Path>, flags: i32, mode: u32) -> RS<SysFile> {
    let file = StdAsyncFile::open(path.as_ref(), flags, mode)
        .map_err(|e| m_error!(EC::IOErr, "open file error", e))?;
    Ok(SysFile::new(Arc::new(file)))
}
