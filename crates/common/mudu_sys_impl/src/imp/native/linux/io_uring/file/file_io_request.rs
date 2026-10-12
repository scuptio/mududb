use super::*;

/// A file IO operation queued for submission to the ring.
pub enum FileIoRequest {
    /// Open a file with openat.
    Open(FileOpenRequest),
    /// Close a file descriptor.
    Close(FileCloseRequest),
    /// Read from a file at an offset.
    Read(FileReadRequest),
    /// Write to a file at an offset.
    Write(FileWriteRequest),
    /// Flush (fsync) a file.
    Flush(FileFlushRequest),
    /// Query a file's length via statx.
    Len(FileLenRequest),
}
impl FileIoRequest {
    /// Returns a short static name of the operation kind for tracing.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Open(_) => "file.open",
            Self::Close(_) => "file.close",
            Self::Read(_) => "file.read",
            Self::Write(_) => "file.write",
            Self::Flush(_) => "file.flush",
            Self::Len(_) => "file.len",
        }
    }
}
