use super::*;

/// A file IO operation that has been submitted to the ring and not yet
/// completed; keeps request-owned buffers alive until the CQE arrives.
pub enum FileInflightOp {
    /// In-flight openat.
    Open(Box<FileOpenRequest>),
    /// In-flight close.
    Close(Box<FileCloseRequest>),
    /// In-flight read; `buf` is the kernel write target.
    Read {
        /// The read request being completed.
        request: Box<FileReadRequest>,
        /// Buffer the kernel reads into; must stay alive until completion.
        buf: Vec<u8>,
    },
    /// In-flight write.
    Write(Box<FileWriteRequest>),
    /// In-flight fsync.
    Flush(Box<FileFlushRequest>),
    /// In-flight statx for file length.
    Len(Box<FileLenRequest>),
}
impl FileInflightOp {
    /// Returns a short static name of the operation kind for tracing.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Open(_) => "file.open",
            Self::Close(_) => "file.close",
            Self::Read { .. } => "file.read",
            Self::Write(_) => "file.write",
            Self::Flush(_) => "file.flush",
            Self::Len(_) => "file.len",
        }
    }
}
