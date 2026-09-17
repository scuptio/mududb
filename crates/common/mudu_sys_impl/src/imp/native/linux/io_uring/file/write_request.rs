use super::*;

pub struct FileWriteRequest {
    fd: RawFd,
    offset: u64,
    data: Vec<u8>,
    written: usize,
    blind_write: bool,
    state: Arc<OpState<usize>>,
}
impl FileWriteRequest {
    pub(crate) fn new(
        fd: RawFd,
        offset: u64,
        data: Vec<u8>,
        blind_write: bool,
        state: Arc<OpState<usize>>,
    ) -> Self {
        Self {
            fd,
            offset,
            data,
            written: 0,
            blind_write,
            state,
        }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn offset(&self) -> u64 {
        self.offset + self.written as u64
    }

    pub fn data_ptr(&self) -> *const libc::c_void {
        unsafe { self.data.as_ptr().add(self.written) as *const libc::c_void }
    }

    pub fn remaining_len(&self) -> usize {
        self.data.len().saturating_sub(self.written)
    }

    pub fn advance(&mut self, written: usize) {
        self.written += written;
    }

    pub fn is_complete(&self) -> bool {
        self.written >= self.data.len()
    }

    pub fn total_len(&self) -> usize {
        self.data.len()
    }

    pub fn blind_write(&self) -> bool {
        self.blind_write
    }

    pub fn finish(self, result: RS<usize>) {
        complete_op(self.state, result);
    }
}
