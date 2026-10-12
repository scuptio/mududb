use super::*;

/// Request to read from a file at an offset via the ring.
pub struct FileReadRequest {
    fd: RawFd,
    len: usize,
    offset: u64,
    state: Arc<OpState<Vec<u8>>>,
}
impl FileReadRequest {
    /// Creates a read of `len` bytes at absolute `offset` on `fd`,
    /// completing into `state`.
    pub fn new(fd: RawFd, len: usize, offset: u64, state: Arc<OpState<Vec<u8>>>) -> Self {
        Self {
            fd,
            len,
            offset,
            state,
        }
    }

    /// Returns the file descriptor to read from.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Returns the number of bytes to read.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if no bytes are requested.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the absolute file offset to read from.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Delivers the read bytes to the waiting task.
    pub fn finish(self, result: RS<Vec<u8>>) {
        complete_op(self.state, result);
    }
}
