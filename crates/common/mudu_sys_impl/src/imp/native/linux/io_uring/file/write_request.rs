use super::*;

/// Request to write data to a file at an offset via the ring; resubmitted
/// on short writes until the whole buffer is written.
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

    /// Returns the file descriptor to write to.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Returns the absolute file offset of the next (re)submission,
    /// accounting for bytes already written.
    pub fn offset(&self) -> u64 {
        self.offset + self.written as u64
    }

    /// Returns a pointer to the first not-yet-written byte; must stay valid
    /// until the CQE arrives.
    pub fn data_ptr(&self) -> *const libc::c_void {
        unsafe { self.data.as_ptr().add(self.written) as *const libc::c_void }
    }

    /// Returns the number of bytes not yet written.
    pub fn remaining_len(&self) -> usize {
        self.data.len().saturating_sub(self.written)
    }

    /// Advances the write cursor after `written` bytes completed.
    pub fn advance(&mut self, written: usize) {
        self.written += written;
    }

    /// Returns `true` once the whole buffer has been written.
    pub fn is_complete(&self) -> bool {
        self.written >= self.data.len()
    }

    /// Returns the total number of bytes to write.
    pub fn total_len(&self) -> usize {
        self.data.len()
    }

    /// Returns whether this write completes optimistically without waiting
    /// for the CQE result.
    pub fn blind_write(&self) -> bool {
        self.blind_write
    }

    /// Delivers the written byte count to the waiting task.
    pub fn finish(self, result: RS<usize>) {
        complete_op(self.state, result);
    }
}
