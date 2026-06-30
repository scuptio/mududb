use super::*;

pub enum FileInflightOp {
    Open(Box<FileOpenRequest>),
    Close(Box<FileCloseRequest>),
    Read {
        request: Box<FileReadRequest>,
        buf: Vec<u8>,
    },
    Write(Box<FileWriteRequest>),
    Flush(Box<FileFlushRequest>),
    Len(Box<FileLenRequest>),
}
impl FileInflightOp {
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
