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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::{
        FileCloseRequest, FileFlushRequest, FileLenRequest, FileOpenRequest, FileReadRequest,
        FileWriteRequest,
    };
    use super::FileInflightOp;
    use crate::imp::io::user_io::op_state;
    use std::ffi::CString;

    #[test]
    fn inflight_op_kind_names() {
        let path = CString::new("/tmp/x").unwrap();
        let open = FileInflightOp::Open(Box::new(FileOpenRequest::new(
            path,
            libc::O_RDONLY,
            0,
            op_state(),
        )));
        assert_eq!(open.kind(), "file.open");
        let close = FileInflightOp::Close(Box::new(FileCloseRequest::new(0, op_state())));
        assert_eq!(close.kind(), "file.close");
        let read = FileInflightOp::Read {
            request: Box::new(FileReadRequest::new(0, 4, 0, op_state())),
            buf: vec![0; 4],
        };
        assert_eq!(read.kind(), "file.read");
        let write = FileInflightOp::Write(Box::new(FileWriteRequest::new(
            0,
            0,
            vec![1],
            false,
            op_state(),
        )));
        assert_eq!(write.kind(), "file.write");
        let flush = FileInflightOp::Flush(Box::new(FileFlushRequest::new(0, (), op_state())));
        assert_eq!(flush.kind(), "file.flush");
        let len = FileInflightOp::Len(Box::new(FileLenRequest::new(0, op_state())));
        assert_eq!(len.kind(), "file.len");
    }
}
