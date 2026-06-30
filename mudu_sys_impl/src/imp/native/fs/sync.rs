use mudu::common::result::RS;
use mudu::error::others::io_error_with_message;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::ffi::OsString;
use std::fmt;
use std::io::ErrorKind;
#[cfg(unix)]
use std::os::fd::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(test)]
use uuid::Uuid;

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

pub(crate) struct StdSyncFile {
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
                .map_err(|e| io_error_with_message(e, "file read_exact_at error"))?;
        }

        #[cfg(not(unix))]
        {
            use std::io::{Read, Seek, SeekFrom};

            let mut file = self
                .inner
                .try_clone()
                .map_err(|e| io_error_with_message(e, "file try_clone error"))?;
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| io_error_with_message(e, "file seek error"))?;
            file.read_exact(&mut buf)
                .map_err(|e| io_error_with_message(e, "file read_exact error"))?;
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
                    .map_err(|e| io_error_with_message(e, "file write_at error"))?;
                if bytes == 0 {
                    return Err(mudu_error!(
                        ErrorCode::WriteZero,
                        "file write_at wrote zero bytes"
                    ));
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
                .map_err(|e| io_error_with_message(e, "file try_clone error"))?;
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| io_error_with_message(e, "file seek error"))?;
            file.write_all(payload)
                .map_err(|e| io_error_with_message(e, "file write_all error"))?;
        }

        Ok(())
    }

    fn fsync(&self) -> RS<()> {
        self.inner
            .sync_all()
            .map_err(|e| io_error_with_message(e, "file fsync error"))
    }

    fn file_len(&self) -> RS<u64> {
        self.inner
            .metadata()
            .map(|metadata| metadata.len())
            .map_err(|e| io_error_with_message(e, "file metadata error"))
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
        std::fs::create_dir_all(path)
            .map_err(|e| io_error_with_message(e, "create directory error"))
    }

    pub fn remove_file(&self, path: &Path) -> RS<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
            Err(err) => Err(io_error_with_message(err, "remove file error")),
        }
    }

    pub fn remove_dir_all(&self, path: &Path) -> RS<()> {
        std::fs::remove_dir_all(path)
            .map_err(|e| io_error_with_message(e, "remove directory error"))
    }

    pub fn read_all(&self, path: &Path) -> RS<Vec<u8>> {
        std::fs::read(path).map_err(|e| io_error_with_message(e, "read file error"))
    }

    pub fn write(&self, path: &Path, data: &[u8]) -> RS<()> {
        std::fs::write(path, data).map_err(|e| io_error_with_message(e, "write file error"))
    }

    pub fn read_to_string(&self, path: &Path) -> RS<String> {
        std::fs::read_to_string(path)
            .map_err(|e| io_error_with_message(e, "read file to string error"))
    }

    pub fn metadata_len(&self, path: &Path) -> RS<u64> {
        std::fs::metadata(path)
            .map_err(|e| io_error_with_message(e, "read file metadata error"))
            .map(|metadata| metadata.len())
    }

    pub fn path_exists(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok()
    }

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

    pub fn copy(&self, from: &Path, to: &Path) -> RS<u64> {
        std::fs::copy(from, to).map_err(|e| io_error_with_message(e, "copy file error"))
    }

    pub fn metadata(&self, path: &Path) -> RS<SMetadata> {
        std::fs::metadata(path)
            .map(SMetadata::from_inner)
            .map_err(|e| io_error_with_message(e, "metadata error"))
    }

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

    pub fn open(&self, path: &Path) -> RS<SFile> {
        std::fs::File::open(path)
            .map(SFile::from_inner)
            .map_err(|e| io_error_with_message(e, "open file error"))
    }

    pub fn open_sys_file(&self, path: &Path) -> RS<SyncSysFile> {
        std::fs::File::open(path)
            .map(SyncSysFile::from_std)
            .map_err(|e| io_error_with_message(e, "open file error"))
    }

    pub fn create(&self, path: &Path) -> RS<SFile> {
        std::fs::File::create(path)
            .map(SFile::from_inner)
            .map_err(|e| io_error_with_message(e, "create file error"))
    }

    pub fn open_with_options(&self, path: &Path, options: &std::fs::OpenOptions) -> RS<SFile> {
        options
            .open(path)
            .map(SFile::from_inner)
            .map_err(|e| io_error_with_message(e, "open file with options error"))
    }

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

pub fn open(path: impl AsRef<Path>) -> RS<SyncSysFile> {
    FsSync::new().open_sys_file(path.as_ref())
}

pub fn open_with_options(
    path: impl AsRef<Path>,
    options: &std::fs::OpenOptions,
) -> RS<SyncSysFile> {
    FsSync::new().open_sys_file_with_options(path.as_ref(), options)
}

// ---------------------------------------------------------------------------
// SFile – synchronous file handle wrapper (no direct std::fs::File exposure)
// ---------------------------------------------------------------------------

pub struct SFile {
    inner: std::fs::File,
}

impl SFile {
    pub fn open(path: impl AsRef<Path>) -> RS<Self> {
        crate::default_sys_io_context()
            .fs_sync()
            .open(path.as_ref())
    }

    pub fn create(path: impl AsRef<Path>) -> RS<Self> {
        crate::default_sys_io_context()
            .fs_sync()
            .create(path.as_ref())
    }

    pub(crate) fn from_inner(inner: std::fs::File) -> Self {
        Self { inner }
    }

    pub fn metadata(&self) -> RS<SMetadata> {
        self.inner
            .metadata()
            .map(SMetadata::from_inner)
            .map_err(|e| io_error_with_message(e, "file metadata error"))
    }

    pub fn sync_all(&self) -> RS<()> {
        self.inner
            .sync_all()
            .map_err(|e| io_error_with_message(e, "file sync_all error"))
    }

    pub fn sync_data(&self) -> RS<()> {
        self.inner
            .sync_data()
            .map_err(|e| io_error_with_message(e, "file sync_data error"))
    }

    pub fn set_len(&self, size: u64) -> RS<()> {
        self.inner
            .set_len(size)
            .map_err(|e| io_error_with_message(e, "file set_len error"))
    }

    pub fn try_clone(&self) -> RS<Self> {
        self.inner
            .try_clone()
            .map(Self::from_inner)
            .map_err(|e| io_error_with_message(e, "file try_clone error"))
    }
}

impl std::io::Read for SFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl std::io::Write for SFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl std::io::Seek for SFile {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

#[cfg(unix)]
impl AsRawFd for SFile {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl fmt::Debug for SFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

// ---------------------------------------------------------------------------
// SOpenOptions – synchronous file open options wrapper
// ---------------------------------------------------------------------------

pub struct SOpenOptions {
    inner: std::fs::OpenOptions,
}

impl SOpenOptions {
    pub fn new() -> Self {
        Self {
            inner: std::fs::OpenOptions::new(),
        }
    }

    pub fn read(&mut self, read: bool) -> &mut Self {
        self.inner.read(read);
        self
    }

    pub fn write(&mut self, write: bool) -> &mut Self {
        self.inner.write(write);
        self
    }

    pub fn append(&mut self, append: bool) -> &mut Self {
        self.inner.append(append);
        self
    }

    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        self.inner.truncate(truncate);
        self
    }

    pub fn create(&mut self, create: bool) -> &mut Self {
        self.inner.create(create);
        self
    }

    pub fn create_new(&mut self, create_new: bool) -> &mut Self {
        self.inner.create_new(create_new);
        self
    }

    pub fn open(&self, path: impl AsRef<Path>) -> RS<SFile> {
        crate::default_sys_io_context()
            .fs_sync()
            .open_with_options(path.as_ref(), self.inner_ref())
    }

    pub(crate) fn inner_ref(&self) -> &std::fs::OpenOptions {
        &self.inner
    }
}

impl Default for SOpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SMetadata – file metadata wrapper
// ---------------------------------------------------------------------------

pub struct SMetadata {
    inner: std::fs::Metadata,
}

impl SMetadata {
    pub(crate) fn from_inner(inner: std::fs::Metadata) -> Self {
        Self { inner }
    }

    pub fn len(&self) -> u64 {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.len() == 0
    }

    pub fn is_file(&self) -> bool {
        self.inner.is_file()
    }

    pub fn is_dir(&self) -> bool {
        self.inner.is_dir()
    }

    pub fn is_symlink(&self) -> bool {
        self.inner.is_symlink()
    }
}

impl fmt::Debug for SMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

// ---------------------------------------------------------------------------
// SDirEntry – directory entry wrapper
// ---------------------------------------------------------------------------

pub struct SDirEntry {
    inner: std::fs::DirEntry,
}

impl SDirEntry {
    pub(crate) fn from_inner(inner: std::fs::DirEntry) -> Self {
        Self { inner }
    }

    pub fn path(&self) -> PathBuf {
        self.inner.path()
    }

    pub fn file_name(&self) -> OsString {
        self.inner.file_name()
    }

    pub fn metadata(&self) -> RS<SMetadata> {
        self.inner
            .metadata()
            .map(|inner| SMetadata { inner })
            .map_err(|e| io_error_with_message(e, "dir entry metadata error"))
    }

    pub fn file_type(&self) -> RS<SFileType> {
        self.inner
            .file_type()
            .map(|inner| SFileType { inner })
            .map_err(|e| io_error_with_message(e, "dir entry file_type error"))
    }
}

// ---------------------------------------------------------------------------
// SFileType – file type wrapper
// ---------------------------------------------------------------------------

pub struct SFileType {
    inner: std::fs::FileType,
}

impl SFileType {
    pub fn is_file(&self) -> bool {
        self.inner.is_file()
    }

    pub fn is_dir(&self) -> bool {
        self.inner.is_dir()
    }

    pub fn is_symlink(&self) -> bool {
        self.inner.is_symlink()
    }
}

impl fmt::Debug for SFileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

// ---------------------------------------------------------------------------
// High-level synchronous filesystem helpers
// ---------------------------------------------------------------------------

pub fn sync_create_dir_all(path: impl AsRef<Path>) -> RS<()> {
    crate::default_sys_io_context()
        .fs_sync()
        .create_dir_all(path.as_ref())
}

pub fn sync_remove_file(path: impl AsRef<Path>) -> RS<()> {
    crate::default_sys_io_context()
        .fs_sync()
        .remove_file(path.as_ref())
}

pub fn sync_remove_dir_all(path: impl AsRef<Path>) -> RS<()> {
    crate::default_sys_io_context()
        .fs_sync()
        .remove_dir_all(path.as_ref())
}

pub fn sync_read_all(path: impl AsRef<Path>) -> RS<Vec<u8>> {
    crate::default_sys_io_context()
        .fs_sync()
        .read_all(path.as_ref())
}

pub fn sync_write(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> RS<()> {
    crate::default_sys_io_context()
        .fs_sync()
        .write(path.as_ref(), data.as_ref())
}

pub fn sync_read_to_string(path: impl AsRef<Path>) -> RS<String> {
    crate::default_sys_io_context()
        .fs_sync()
        .read_to_string(path.as_ref())
}

pub fn sync_path_exists(path: impl AsRef<Path>) -> bool {
    crate::default_sys_io_context()
        .fs_sync()
        .path_exists(path.as_ref())
}

pub fn sync_read_dir(path: impl AsRef<Path>) -> RS<Vec<PathBuf>> {
    crate::default_sys_io_context()
        .fs_sync()
        .read_dir(path.as_ref())
}

pub fn create_dir_all(path: impl AsRef<Path>) -> RS<()> {
    sync_create_dir_all(path)
}

pub fn remove_file(path: impl AsRef<Path>) -> RS<()> {
    sync_remove_file(path)
}

pub fn remove_dir_all(path: impl AsRef<Path>) -> RS<()> {
    sync_remove_dir_all(path)
}

pub fn read(path: impl AsRef<Path>) -> RS<Vec<u8>> {
    sync_read_all(path)
}

pub fn write(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> RS<()> {
    sync_write(path, data)
}

pub fn read_to_string(path: impl AsRef<Path>) -> RS<String> {
    sync_read_to_string(path)
}

pub fn path_exists(path: impl AsRef<Path>) -> bool {
    sync_path_exists(path)
}

pub fn read_dir(path: impl AsRef<Path>) -> RS<Vec<PathBuf>> {
    sync_read_dir(path)
}

pub fn sync_read_dir_entries(path: impl AsRef<Path>) -> RS<Vec<SDirEntry>> {
    crate::default_sys_io_context()
        .fs_sync()
        .read_dir_entries(path.as_ref())
}

pub fn read_dir_entries(path: impl AsRef<Path>) -> RS<Vec<SDirEntry>> {
    sync_read_dir_entries(path)
}

pub fn sync_copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> RS<u64> {
    crate::default_sys_io_context()
        .fs_sync()
        .copy(from.as_ref(), to.as_ref())
}

pub fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> RS<u64> {
    sync_copy(from, to)
}

pub fn sync_metadata(path: impl AsRef<Path>) -> RS<SMetadata> {
    crate::default_sys_io_context()
        .fs_sync()
        .metadata(path.as_ref())
}

pub fn metadata(path: impl AsRef<Path>) -> RS<SMetadata> {
    sync_metadata(path)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::default_constructed_unit_structs
)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom, Write};

    fn tmp_dir() -> PathBuf {
        let dir = PathBuf::from("target/tmp").join(format!("sync-fs-{}", Uuid::new_v4()));
        let fs = FsSync::new();
        fs.create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &Path) {
        let fs = FsSync::new();
        let _ = fs.remove_dir_all(dir);
    }

    #[test]
    fn fs_sync_new_and_default() {
        let _ = FsSync::new();
        let _ = FsSync::default();
    }

    #[test]
    fn fs_sync_create_remove_dir_all_and_path_exists() {
        let fs = FsSync::new();
        let dir = tmp_dir();
        assert!(fs.path_exists(&dir));
        fs.remove_dir_all(&dir).unwrap();
        assert!(!fs.path_exists(&dir));
    }

    #[test]
    fn fs_sync_write_read_all_read_to_string_roundtrip() {
        let fs = FsSync::new();
        let dir = tmp_dir();
        let path = dir.join("file.txt");
        fs.write(&path, b"hello world").unwrap();
        assert_eq!(fs.read_all(&path).unwrap(), b"hello world");
        assert_eq!(fs.read_to_string(&path).unwrap(), "hello world");
        cleanup(&dir);
    }

    #[test]
    fn fs_sync_metadata_len_and_metadata() {
        let fs = FsSync::new();
        let dir = tmp_dir();
        let file = dir.join("data.bin");
        fs.write(&file, b"payload").unwrap();
        assert_eq!(fs.metadata_len(&file).unwrap(), 7);
        let meta = fs.metadata(&file).unwrap();
        assert!(meta.is_file());
        assert!(!meta.is_dir());
        assert!(!meta.is_empty());
        assert_eq!(meta.len(), 7);

        let empty = dir.join("empty");
        fs.write(&empty, b"").unwrap();
        assert!(fs.metadata(&empty).unwrap().is_empty());

        let subdir = dir.join("subdir");
        fs.create_dir_all(&subdir).unwrap();
        let dir_meta = fs.metadata(&subdir).unwrap();
        assert!(dir_meta.is_dir());
        assert!(!dir_meta.is_file());
        cleanup(&dir);
    }

    #[test]
    fn fs_sync_read_dir_and_read_dir_entries() {
        let fs = FsSync::new();
        let dir = tmp_dir();
        fs.write(&dir.join("a.txt"), b"a").unwrap();
        fs.write(&dir.join("b.txt"), b"b").unwrap();

        let paths = fs.read_dir(&dir).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|p| p.file_name().unwrap() == "a.txt"));
        assert!(paths.iter().any(|p| p.file_name().unwrap() == "b.txt"));

        let entries = fs.read_dir_entries(&dir).unwrap();
        assert_eq!(entries.len(), 2);
        let mut names: Vec<_> = entries.iter().map(|e| e.file_name()).collect();
        names.sort();
        assert_eq!(names, vec!["a.txt", "b.txt"]);
        for entry in &entries {
            assert!(entry.path().starts_with(&dir));
            assert!(entry.file_type().unwrap().is_file());
            assert!(entry.metadata().unwrap().is_file());
        }
        cleanup(&dir);
    }

    #[test]
    fn fs_sync_remove_file_idempotent() {
        let fs = FsSync::new();
        let dir = tmp_dir();
        let path = dir.join("to-remove.txt");
        fs.remove_file(&path).unwrap();
        fs.write(&path, b"x").unwrap();
        fs.remove_file(&path).unwrap();
        assert!(!fs.path_exists(&path));
        fs.remove_file(&path).unwrap();
        cleanup(&dir);
    }

    // Miri does not support the copy_file_range syscall that std::fs::copy uses.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn fs_sync_copy_file() {
        let fs = FsSync::new();
        let dir = tmp_dir();
        let src = dir.join("src.txt");
        let dst = dir.join("dst.txt");
        fs.write(&src, b"copy me").unwrap();
        let copied = fs.copy(&src, &dst).unwrap();
        assert_eq!(copied, 7);
        assert_eq!(fs.read_all(&dst).unwrap(), b"copy me");
        cleanup(&dir);
    }

    #[test]
    fn fs_sync_open_create_and_options() {
        let fs = FsSync::new();
        let dir = tmp_dir();
        let path = dir.join("created.txt");
        {
            let mut file = fs.create(&path).unwrap();
            file.write_all(b"created").unwrap();
        }
        {
            let mut file = fs.open(&path).unwrap();
            let mut buf = String::new();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!(buf, "created");
        }

        let path2 = dir.join("options.txt");
        let mut opts = std::fs::OpenOptions::new();
        opts.read(true).write(true).create(true).truncate(true);
        let mut file = fs.open_with_options(&path2, &opts).unwrap();
        file.write_all(b"options").unwrap();
        drop(file);
        assert_eq!(fs.read_all(&path2).unwrap(), b"options");

        let mut sopts = SOpenOptions::new();
        sopts.read(true).write(true).create(true).truncate(true);
        let mut file = sopts.open(dir.join("soptions.txt")).unwrap();
        file.write_all(b"soptions").unwrap();
        drop(file);
        assert_eq!(fs.read_all(&dir.join("soptions.txt")).unwrap(), b"soptions");
        cleanup(&dir);
    }

    #[test]
    fn sync_sys_file_positioned_io() {
        let fs = FsSync::new();
        let dir = tmp_dir();
        let path = dir.join("sysfile.bin");
        let mut std_opts = std::fs::OpenOptions::new();
        std_opts.read(true).write(true).create(true).truncate(true);
        let sys_file = fs.open_sys_file_with_options(&path, &std_opts).unwrap();
        sys_file.write_all_at(0, b"hello").unwrap();
        sys_file.write_all_at(5, b" world").unwrap();
        sys_file.fsync().unwrap();
        assert_eq!(sys_file.file_len().unwrap(), 11);
        assert_eq!(sys_file.read_exact_at(0, 5).unwrap(), b"hello");
        assert_eq!(sys_file.read_exact_at(6, 5).unwrap(), b"world");
        sys_file.close().unwrap();
        cleanup(&dir);
    }

    #[test]
    fn sfile_read_write_seek() {
        let dir = tmp_dir();
        let path = dir.join("sfile.bin");
        {
            let mut file = SFile::create(&path).unwrap();
            file.write_all(b"abc").unwrap();
            file.write_all(b"def").unwrap();
        }
        {
            let mut file = SFile::open(&path).unwrap();
            file.seek(SeekFrom::Start(3)).unwrap();
            let mut buf = [0u8; 3];
            file.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"def");
        }
        cleanup(&dir);
    }

    #[test]
    fn sfile_sync_all_sync_data_set_len_try_clone() {
        let dir = tmp_dir();
        let path = dir.join("sfile2.bin");
        let mut file = SFile::create(&path).unwrap();
        file.write_all(b"0123456789").unwrap();
        file.sync_all().unwrap();
        file.sync_data().unwrap();
        file.set_len(4).unwrap();
        drop(file);
        assert_eq!(FsSync::new().metadata_len(&path).unwrap(), 4);

        let file = SFile::open(&path).unwrap();
        let clone = file.try_clone().unwrap();
        drop(file);
        let mut buf = String::new();
        let mut clone = clone;
        clone.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "0123");
        cleanup(&dir);
    }

    #[test]
    fn sopen_options_builder_chain() {
        let dir = tmp_dir();
        let path = dir.join("chain.txt");
        let mut file = SOpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        file.write_all(b"chain").unwrap();
        drop(file);
        assert_eq!(FsSync::new().read_all(&path).unwrap(), b"chain");
        cleanup(&dir);
    }

    #[test]
    fn smetadata_sfile_type_sdirentry_behavior() {
        let fs = FsSync::new();
        let dir = tmp_dir();
        let file = dir.join("meta.txt");
        fs.write(&file, b"data").unwrap();

        let meta = fs.metadata(&file).unwrap();
        assert!(meta.is_file());
        assert!(!meta.is_dir());
        assert!(!meta.is_symlink());
        assert_eq!(meta.len(), 4);

        let entries = fs.read_dir_entries(&dir).unwrap();
        let entry = entries.into_iter().next().unwrap();
        let ft = entry.file_type().unwrap();
        assert!(ft.is_file());
        assert!(!ft.is_dir());
        assert!(!ft.is_symlink());
        let entry_meta = entry.metadata().unwrap();
        assert!(entry_meta.is_file());
        cleanup(&dir);
    }

    // Miri does not support the copy_file_range syscall that std::fs::copy uses.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn top_level_helpers_exposed() {
        let dir = tmp_dir();
        let path = dir.join("top.txt");
        super::create_dir_all(dir.join("nested")).unwrap();
        super::write(&path, b"top").unwrap();
        assert!(super::path_exists(&path));
        assert_eq!(super::read(&path).unwrap(), b"top");
        assert_eq!(super::read_to_string(&path).unwrap(), "top");
        let copied_path = dir.join("top-copy.txt");
        assert_eq!(super::copy(&path, &copied_path).unwrap(), 3);
        let meta = super::metadata(&path).unwrap();
        assert!(meta.is_file());
        let paths = super::read_dir(&dir).unwrap();
        assert_eq!(paths.len(), 3);
        let entries = super::read_dir_entries(&dir).unwrap();
        assert_eq!(entries.len(), 3);

        let opt_path = dir.join("opt.bin");
        let mut opts = std::fs::OpenOptions::new();
        opts.read(true).write(true).create(true).truncate(true);
        let file = super::open_with_options(&opt_path, &opts).unwrap();
        file.write_all_at(0, b"opts").unwrap();
        drop(file);
        let file = super::open(&opt_path).unwrap();
        let buf = file.read_exact_at(0, 4).unwrap();
        assert_eq!(buf, b"opts");

        super::remove_file(&path).unwrap();
        super::remove_dir_all(dir.join("nested")).unwrap();
        super::remove_dir_all(&dir).unwrap();
        assert!(!super::path_exists(&dir));
        cleanup(&dir);
    }
}

#[cfg(test)]
#[path = "sync_test.rs"]
mod sync_test;
