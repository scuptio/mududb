use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::io::ErrorKind;
#[cfg(unix)]
use std::os::fd::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct File {
    inner: std::fs::File,
}

impl File {
    pub fn from_inner(inner: std::fs::File) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> std::fs::File {
        self.inner
    }
}

pub struct Metadata {
    inner: std::fs::Metadata,
}

impl Metadata {
    pub fn from_inner(inner: std::fs::Metadata) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> std::fs::Metadata {
        self.inner
    }
}

pub struct DirEntry {
    inner: std::fs::DirEntry,
}

impl DirEntry {
    pub fn from_inner(inner: std::fs::DirEntry) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> std::fs::DirEntry {
        self.inner
    }
}

pub trait SyncFile: Send + Sync {
    fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>>;
    fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()>;
    fn fsync(&self) -> RS<()>;
    fn file_len(&self) -> RS<u64>;
    fn close(&self) -> RS<()>;

    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<RawFd>;
}

#[derive(Clone)]
pub struct SyncSysFile {
    inner: Arc<dyn SyncFile>,
}

impl SyncSysFile {
    pub fn new(inner: Arc<dyn SyncFile>) -> Self {
        Self { inner }
    }

    pub fn from_std(file: std::fs::File) -> Self {
        Self::new(Arc::new(StdSyncFile::new(file)))
    }

    pub fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        self.inner.read_exact_at(offset, len)
    }

    pub fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        self.inner.write_all_at(offset, payload)
    }

    pub fn fsync(&self) -> RS<()> {
        self.inner.fsync()
    }

    pub fn file_len(&self) -> RS<u64> {
        self.inner.file_len()
    }

    pub fn close(&self) -> RS<()> {
        self.inner.close()
    }

    #[cfg(unix)]
    pub fn as_raw_fd(&self) -> Option<RawFd> {
        self.inner.as_raw_fd()
    }
}

pub struct StdSyncFile {
    inner: std::fs::File,
}

impl StdSyncFile {
    pub fn new(inner: std::fs::File) -> Self {
        Self { inner }
    }
}

impl SyncFile for StdSyncFile {
    fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        let mut buf = vec![0; len];

        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;

            self.inner
                .read_exact_at(&mut buf, offset)
                .map_err(|e| m_error!(EC::IOErr, "file read_exact_at error", e))?;
        }

        #[cfg(not(unix))]
        {
            use std::io::{Read, Seek, SeekFrom};

            let mut file = self
                .inner
                .try_clone()
                .map_err(|e| m_error!(EC::IOErr, "file try_clone error", e))?;
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| m_error!(EC::IOErr, "file seek error", e))?;
            file.read_exact(&mut buf)
                .map_err(|e| m_error!(EC::IOErr, "file read_exact error", e))?;
        }

        Ok(buf)
    }

    fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;

            let mut written = 0;
            while written < payload.len() {
                let bytes = self
                    .inner
                    .write_at(&payload[written..], offset + written as u64)
                    .map_err(|e| m_error!(EC::IOErr, "file write_at error", e))?;
                if bytes == 0 {
                    return Err(m_error!(EC::IOErr, "file write_at wrote zero bytes"));
                }
                written += bytes;
            }
        }

        #[cfg(not(unix))]
        {
            use std::io::{Seek, SeekFrom, Write};

            let mut file = self
                .inner
                .try_clone()
                .map_err(|e| m_error!(EC::IOErr, "file try_clone error", e))?;
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| m_error!(EC::IOErr, "file seek error", e))?;
            file.write_all(payload)
                .map_err(|e| m_error!(EC::IOErr, "file write_all error", e))?;
        }

        Ok(())
    }

    fn fsync(&self) -> RS<()> {
        self.inner
            .sync_all()
            .map_err(|e| m_error!(EC::IOErr, "file fsync error", e))
    }

    fn file_len(&self) -> RS<u64> {
        self.inner
            .metadata()
            .map(|metadata| metadata.len())
            .map_err(|e| m_error!(EC::IOErr, "file metadata error", e))
    }

    fn close(&self) -> RS<()> {
        Ok(())
    }

    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<RawFd> {
        Some(self.inner.as_raw_fd())
    }
}

/// Synchronous filesystem operations — native implementation.
pub struct FsSync;

impl Default for FsSync {
    fn default() -> Self {
        Self::new()
    }
}

impl FsSync {
    pub fn new() -> Self {
        Self
    }

    pub fn create_dir_all(&self, path: &Path) -> RS<()> {
        std::fs::create_dir_all(path).map_err(|e| m_error!(EC::IOErr, "create directory error", e))
    }

    pub fn remove_file(&self, path: &Path) -> RS<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
            Err(err) => Err(m_error!(EC::IOErr, "remove file error", err)),
        }
    }

    pub fn remove_dir_all(&self, path: &Path) -> RS<()> {
        std::fs::remove_dir_all(path).map_err(|e| m_error!(EC::IOErr, "remove directory error", e))
    }

    pub fn read_all(&self, path: &Path) -> RS<Vec<u8>> {
        std::fs::read(path).map_err(|e| m_error!(EC::IOErr, "read file error", e))
    }

    pub fn write(&self, path: &Path, data: &[u8]) -> RS<()> {
        std::fs::write(path, data).map_err(|e| m_error!(EC::IOErr, "write file error", e))
    }

    pub fn read_to_string(&self, path: &Path) -> RS<String> {
        std::fs::read_to_string(path)
            .map_err(|e| m_error!(EC::IOErr, "read file to string error", e))
    }

    pub fn metadata_len(&self, path: &Path) -> RS<u64> {
        std::fs::metadata(path)
            .map_err(|e| m_error!(EC::IOErr, "read file metadata error", e))
            .map(|metadata| metadata.len())
    }

    pub fn path_exists(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok()
    }

    pub fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in
            std::fs::read_dir(path).map_err(|e| m_error!(EC::IOErr, "read directory error", e))?
        {
            let entry = entry.map_err(|e| m_error!(EC::IOErr, "read directory entry error", e))?;
            paths.push(entry.path());
        }
        Ok(paths)
    }

    pub fn copy(&self, from: &Path, to: &Path) -> RS<u64> {
        std::fs::copy(from, to).map_err(|e| m_error!(EC::IOErr, "copy file error", e))
    }

    pub fn metadata(&self, path: &Path) -> RS<Metadata> {
        std::fs::metadata(path)
            .map(Metadata::from_inner)
            .map_err(|e| m_error!(EC::IOErr, "metadata error", e))
    }

    pub fn read_dir_entries(&self, path: &Path) -> RS<Vec<DirEntry>> {
        let mut entries = Vec::new();
        for entry in
            std::fs::read_dir(path).map_err(|e| m_error!(EC::IOErr, "read directory error", e))?
        {
            let entry = entry.map_err(|e| m_error!(EC::IOErr, "read directory entry error", e))?;
            entries.push(DirEntry::from_inner(entry));
        }
        Ok(entries)
    }

    pub fn open(&self, path: &Path) -> RS<File> {
        std::fs::File::open(path)
            .map(File::from_inner)
            .map_err(|e| m_error!(EC::IOErr, "open file error", e))
    }

    pub fn open_sys_file(&self, path: &Path) -> RS<SyncSysFile> {
        std::fs::File::open(path)
            .map(SyncSysFile::from_std)
            .map_err(|e| m_error!(EC::IOErr, "open file error", e))
    }

    pub fn create(&self, path: &Path) -> RS<File> {
        std::fs::File::create(path)
            .map(File::from_inner)
            .map_err(|e| m_error!(EC::IOErr, "create file error", e))
    }

    pub fn open_with_options(&self, path: &Path, options: &std::fs::OpenOptions) -> RS<File> {
        options
            .open(path)
            .map(File::from_inner)
            .map_err(|e| m_error!(EC::IOErr, "open file with options error", e))
    }

    pub fn open_sys_file_with_options(
        &self,
        path: &Path,
        options: &std::fs::OpenOptions,
    ) -> RS<SyncSysFile> {
        options
            .open(path)
            .map(SyncSysFile::from_std)
            .map_err(|e| m_error!(EC::IOErr, "open file with options error", e))
    }
}
