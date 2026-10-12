use super::file::SFile;
use mudu::common::result::RS;
use std::path::Path;

/// Builder-style file open options, mirroring [`std::fs::OpenOptions`].
pub struct SOpenOptions {
    inner: std::fs::OpenOptions,
}

impl SOpenOptions {
    /// Creates blank open options (everything disabled).
    pub fn new() -> Self {
        Self {
            inner: std::fs::OpenOptions::new(),
        }
    }

    /// Enables or disables read access.
    pub fn read(&mut self, read: bool) -> &mut Self {
        self.inner.read(read);
        self
    }

    /// Enables or disables write access.
    pub fn write(&mut self, write: bool) -> &mut Self {
        self.inner.write(write);
        self
    }

    /// Enables or disables append mode.
    pub fn append(&mut self, append: bool) -> &mut Self {
        self.inner.append(append);
        self
    }

    /// Enables or disables truncating the file on open.
    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        self.inner.truncate(truncate);
        self
    }

    /// Enables or disables creating the file if it does not exist.
    pub fn create(&mut self, create: bool) -> &mut Self {
        self.inner.create(create);
        self
    }

    /// Enables or disables failing the open unless the file is newly
    /// created.
    pub fn create_new(&mut self, create_new: bool) -> &mut Self {
        self.inner.create_new(create_new);
        self
    }

    /// Opens the file at `path` with these options using the default IO
    /// context.
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
