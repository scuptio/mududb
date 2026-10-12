use mudu::common::result::RS;
use mudu::error::others::io_error_with_message;
use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

/// File metadata snapshot.
pub struct SMetadata {
    inner: std::fs::Metadata,
}

impl SMetadata {
    pub(crate) fn from_inner(inner: std::fs::Metadata) -> Self {
        Self { inner }
    }

    /// Returns the length in bytes of the file.
    pub fn len(&self) -> u64 {
        self.inner.len()
    }

    /// Returns `true` if the file has zero length.
    pub fn is_empty(&self) -> bool {
        self.inner.len() == 0
    }

    /// Returns `true` if this metadata describes a regular file.
    pub fn is_file(&self) -> bool {
        self.inner.is_file()
    }

    /// Returns `true` if this metadata describes a directory.
    pub fn is_dir(&self) -> bool {
        self.inner.is_dir()
    }

    /// Returns `true` if this metadata describes a symbolic link.
    pub fn is_symlink(&self) -> bool {
        self.inner.is_symlink()
    }
}

impl fmt::Debug for SMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

/// A single entry of a directory listing.
pub struct SDirEntry {
    inner: std::fs::DirEntry,
}

impl SDirEntry {
    pub(crate) fn from_inner(inner: std::fs::DirEntry) -> Self {
        Self { inner }
    }

    /// Returns the full path of this directory entry.
    pub fn path(&self) -> PathBuf {
        self.inner.path()
    }

    /// Returns the bare file name of this directory entry.
    pub fn file_name(&self) -> OsString {
        self.inner.file_name()
    }

    /// Returns the metadata of this entry (follows symlinks).
    pub fn metadata(&self) -> RS<SMetadata> {
        self.inner
            .metadata()
            .map(|inner| SMetadata { inner })
            .map_err(|e| io_error_with_message(e, "dir entry metadata error"))
    }

    /// Returns the file type of this entry (does not follow symlinks).
    pub fn file_type(&self) -> RS<SFileType> {
        self.inner
            .file_type()
            .map(|inner| SFileType { inner })
            .map_err(|e| io_error_with_message(e, "dir entry file_type error"))
    }
}

/// File type of a directory entry.
pub struct SFileType {
    inner: std::fs::FileType,
}

impl SFileType {
    /// Returns `true` if the entry is a regular file.
    pub fn is_file(&self) -> bool {
        self.inner.is_file()
    }

    /// Returns `true` if the entry is a directory.
    pub fn is_dir(&self) -> bool {
        self.inner.is_dir()
    }

    /// Returns `true` if the entry is a symbolic link.
    pub fn is_symlink(&self) -> bool {
        self.inner.is_symlink()
    }
}

impl fmt::Debug for SFileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}
