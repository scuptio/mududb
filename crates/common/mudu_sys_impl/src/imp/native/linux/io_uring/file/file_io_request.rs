use super::*;

pub enum FileIoRequest {
    Open(FileOpenRequest),
    Close(FileCloseRequest),
    Read(FileReadRequest),
    Write(FileWriteRequest),
    Flush(FileFlushRequest),
    Len(FileLenRequest),
}
impl FileIoRequest {
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
