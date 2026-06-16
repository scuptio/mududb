pub use crate::imp::fs_sync::{DirEntry, File, Metadata, SyncSysFile};
use mudu::common::result::RS;
use std::path::Path;

pub fn open(path: impl AsRef<Path>) -> RS<SyncSysFile> {
    crate::imp::env::Sys::fs_sync().open_sys_file(path.as_ref())
}

pub fn open_with_options(
    path: impl AsRef<Path>,
    options: &std::fs::OpenOptions,
) -> RS<SyncSysFile> {
    crate::imp::env::Sys::fs_sync().open_sys_file_with_options(path.as_ref(), options)
}
