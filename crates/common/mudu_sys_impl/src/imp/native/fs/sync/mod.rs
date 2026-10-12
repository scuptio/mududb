mod file;
mod metadata;
mod options;
mod positioned;

pub use file::SFile;
pub use metadata::{SDirEntry, SFileType, SMetadata};
pub use options::SOpenOptions;
pub use positioned::{SyncFile, SyncSysFile};

use mudu::common::result::RS;
use mudu::error::others::io_error_with_message;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Synchronous filesystem operations — native implementation.
pub struct FsSync;

impl Default for FsSync {
    fn default() -> Self {
        Self::new()
    }
}

impl FsSync {
    /// Creates the synchronous filesystem facade.
    pub fn new() -> Self {
        Self
    }

    /// Creates `path` and all missing parent directories.
    pub fn create_dir_all(&self, path: &Path) -> RS<()> {
        std::fs::create_dir_all(path)
            .map_err(|e| io_error_with_message(e, "create directory error"))
    }

    /// Removes the file at `path`; succeeds silently if it does not exist.
    pub fn remove_file(&self, path: &Path) -> RS<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
            Err(err) => Err(io_error_with_message(err, "remove file error")),
        }
    }

    /// Removes the directory at `path` and all of its contents.
    pub fn remove_dir_all(&self, path: &Path) -> RS<()> {
        std::fs::remove_dir_all(path)
            .map_err(|e| io_error_with_message(e, "remove directory error"))
    }

    /// Reads the entire contents of the file at `path`.
    pub fn read_all(&self, path: &Path) -> RS<Vec<u8>> {
        std::fs::read(path).map_err(|e| io_error_with_message(e, "read file error"))
    }

    /// Writes `data` as the entire contents of the file at `path`, creating
    /// or truncating it.
    pub fn write(&self, path: &Path, data: &[u8]) -> RS<()> {
        std::fs::write(path, data).map_err(|e| io_error_with_message(e, "write file error"))
    }

    /// Reads the entire contents of the file at `path` as a UTF-8 string.
    pub fn read_to_string(&self, path: &Path) -> RS<String> {
        std::fs::read_to_string(path)
            .map_err(|e| io_error_with_message(e, "read file to string error"))
    }

    /// Returns the length in bytes of the file at `path`.
    pub fn metadata_len(&self, path: &Path) -> RS<u64> {
        std::fs::metadata(path)
            .map_err(|e| io_error_with_message(e, "read file metadata error"))
            .map(|metadata| metadata.len())
    }

    /// Returns `true` if a filesystem object exists at `path`.
    pub fn path_exists(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok()
    }

    /// Lists the paths of all entries in the directory at `path`.
    pub fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in
            std::fs::read_dir(path).map_err(|e| io_error_with_message(e, "read directory error"))?
        {
            let entry =
                entry.map_err(|e| io_error_with_message(e, "read directory entry error"))?;
            paths.push(entry.path());
        }
        Ok(paths)
    }

    /// Copies the file at `from` to `to`, returning the number of bytes
    /// copied.
    pub fn copy(&self, from: &Path, to: &Path) -> RS<u64> {
        std::fs::copy(from, to).map_err(|e| io_error_with_message(e, "copy file error"))
    }

    /// Returns the metadata of the filesystem object at `path`.
    pub fn metadata(&self, path: &Path) -> RS<SMetadata> {
        std::fs::metadata(path)
            .map(SMetadata::from_inner)
            .map_err(|e| io_error_with_message(e, "metadata error"))
    }

    /// Lists the entries of the directory at `path` as [`SDirEntry`] values.
    pub fn read_dir_entries(&self, path: &Path) -> RS<Vec<SDirEntry>> {
        let mut entries = Vec::new();
        for entry in
            std::fs::read_dir(path).map_err(|e| io_error_with_message(e, "read directory error"))?
        {
            let entry =
                entry.map_err(|e| io_error_with_message(e, "read directory entry error"))?;
            entries.push(SDirEntry::from_inner(entry));
        }
        Ok(entries)
    }

    /// Opens the file at `path` read-only.
    pub fn open(&self, path: &Path) -> RS<SFile> {
        std::fs::File::open(path)
            .map(SFile::from_inner)
            .map_err(|e| io_error_with_message(e, "open file error"))
    }

    /// Opens the file at `path` read-only as a positioned-IO handle.
    pub fn open_sys_file(&self, path: &Path) -> RS<SyncSysFile> {
        std::fs::File::open(path)
            .map(SyncSysFile::from_std)
            .map_err(|e| io_error_with_message(e, "open file error"))
    }

    /// Creates (or truncates) the file at `path` and opens it write-only.
    pub fn create(&self, path: &Path) -> RS<SFile> {
        std::fs::File::create(path)
            .map(SFile::from_inner)
            .map_err(|e| io_error_with_message(e, "create file error"))
    }

    /// Opens the file at `path` with the given [`std::fs::OpenOptions`].
    pub fn open_with_options(&self, path: &Path, options: &std::fs::OpenOptions) -> RS<SFile> {
        options
            .open(path)
            .map(SFile::from_inner)
            .map_err(|e| io_error_with_message(e, "open file with options error"))
    }

    /// Opens the file at `path` with the given [`std::fs::OpenOptions`] as a
    /// positioned-IO handle.
    pub fn open_sys_file_with_options(
        &self,
        path: &Path,
        options: &std::fs::OpenOptions,
    ) -> RS<SyncSysFile> {
        options
            .open(path)
            .map(SyncSysFile::from_std)
            .map_err(|e| io_error_with_message(e, "open file with options error"))
    }
}

/// Opens the file at `path` read-only as a positioned-IO handle using the
/// default IO context.
pub fn open(path: impl AsRef<Path>) -> RS<SyncSysFile> {
    FsSync::new().open_sys_file(path.as_ref())
}

/// Opens the file at `path` with the given [`std::fs::OpenOptions`] as a
/// positioned-IO handle using the default IO context.
pub fn open_with_options(
    path: impl AsRef<Path>,
    options: &std::fs::OpenOptions,
) -> RS<SyncSysFile> {
    FsSync::new().open_sys_file_with_options(path.as_ref(), options)
}

// ---------------------------------------------------------------------------
// High-level synchronous filesystem helpers
// ---------------------------------------------------------------------------

/// Creates `path` and all missing parent directories using the default IO
/// context.
pub fn sync_create_dir_all(path: impl AsRef<Path>) -> RS<()> {
    crate::default_sys_io_context()
        .fs_sync()
        .create_dir_all(path.as_ref())
}

/// Removes the file at `path` using the default IO context; succeeds
/// silently if it does not exist.
pub fn sync_remove_file(path: impl AsRef<Path>) -> RS<()> {
    crate::default_sys_io_context()
        .fs_sync()
        .remove_file(path.as_ref())
}

/// Removes the directory at `path` and all of its contents using the
/// default IO context.
pub fn sync_remove_dir_all(path: impl AsRef<Path>) -> RS<()> {
    crate::default_sys_io_context()
        .fs_sync()
        .remove_dir_all(path.as_ref())
}

/// Reads the entire contents of the file at `path` using the default IO
/// context.
pub fn sync_read_all(path: impl AsRef<Path>) -> RS<Vec<u8>> {
    crate::default_sys_io_context()
        .fs_sync()
        .read_all(path.as_ref())
}

/// Writes `data` as the entire contents of the file at `path` using the
/// default IO context.
pub fn sync_write(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> RS<()> {
    crate::default_sys_io_context()
        .fs_sync()
        .write(path.as_ref(), data.as_ref())
}

/// Reads the entire contents of the file at `path` as a UTF-8 string using
/// the default IO context.
pub fn sync_read_to_string(path: impl AsRef<Path>) -> RS<String> {
    crate::default_sys_io_context()
        .fs_sync()
        .read_to_string(path.as_ref())
}

/// Returns `true` if a filesystem object exists at `path` (default IO
/// context).
pub fn sync_path_exists(path: impl AsRef<Path>) -> bool {
    crate::default_sys_io_context()
        .fs_sync()
        .path_exists(path.as_ref())
}

/// Lists the paths of all entries in the directory at `path` using the
/// default IO context.
pub fn sync_read_dir(path: impl AsRef<Path>) -> RS<Vec<PathBuf>> {
    crate::default_sys_io_context()
        .fs_sync()
        .read_dir(path.as_ref())
}

/// Alias of [`sync_create_dir_all`].
pub fn create_dir_all(path: impl AsRef<Path>) -> RS<()> {
    sync_create_dir_all(path)
}

/// Alias of [`sync_remove_file`].
pub fn remove_file(path: impl AsRef<Path>) -> RS<()> {
    sync_remove_file(path)
}

/// Alias of [`sync_remove_dir_all`].
pub fn remove_dir_all(path: impl AsRef<Path>) -> RS<()> {
    sync_remove_dir_all(path)
}

/// Alias of [`sync_read_all`].
pub fn read(path: impl AsRef<Path>) -> RS<Vec<u8>> {
    sync_read_all(path)
}

/// Alias of [`sync_write`].
pub fn write(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> RS<()> {
    sync_write(path, data)
}

/// Alias of [`sync_read_to_string`].
pub fn read_to_string(path: impl AsRef<Path>) -> RS<String> {
    sync_read_to_string(path)
}

/// Alias of [`sync_path_exists`].
pub fn path_exists(path: impl AsRef<Path>) -> bool {
    sync_path_exists(path)
}

/// Alias of [`sync_read_dir`].
pub fn read_dir(path: impl AsRef<Path>) -> RS<Vec<PathBuf>> {
    sync_read_dir(path)
}

/// Lists the entries of the directory at `path` as [`SDirEntry`] values
/// using the default IO context.
pub fn sync_read_dir_entries(path: impl AsRef<Path>) -> RS<Vec<SDirEntry>> {
    crate::default_sys_io_context()
        .fs_sync()
        .read_dir_entries(path.as_ref())
}

/// Alias of [`sync_read_dir_entries`].
pub fn read_dir_entries(path: impl AsRef<Path>) -> RS<Vec<SDirEntry>> {
    sync_read_dir_entries(path)
}

/// Copies the file at `from` to `to` using the default IO context,
/// returning the number of bytes copied.
pub fn sync_copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> RS<u64> {
    crate::default_sys_io_context()
        .fs_sync()
        .copy(from.as_ref(), to.as_ref())
}

/// Alias of [`sync_copy`].
pub fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> RS<u64> {
    sync_copy(from, to)
}

/// Returns the metadata of the filesystem object at `path` using the
/// default IO context.
pub fn sync_metadata(path: impl AsRef<Path>) -> RS<SMetadata> {
    crate::default_sys_io_context()
        .fs_sync()
        .metadata(path.as_ref())
}

/// Alias of [`sync_metadata`].
pub fn metadata(path: impl AsRef<Path>) -> RS<SMetadata> {
    sync_metadata(path)
}

#[cfg(test)]
#[path = "sync_test.rs"]
mod sync_test;
