use super::*;

pub struct FileReadRequest {
    fd: RawFd,
    len: usize,
    offset: u64,
    state: Arc<OpState<Vec<u8>>>,
}
impl FileReadRequest {
    pub fn new(fd: RawFd, len: usize, offset: u64, state: Arc<OpState<Vec<u8>>>) -> Self {
        Self {
            fd,
            len,
            offset,
            state,
        }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn finish(self, result: RS<Vec<u8>>) {
        complete_op(self.state, result);
    }
}
