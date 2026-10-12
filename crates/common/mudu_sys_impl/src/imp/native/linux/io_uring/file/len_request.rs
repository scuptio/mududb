use super::*;

/// Request to query a file's length via statx on the ring.
pub struct FileLenRequest {
    fd: RawFd,
    pub(crate) statx: Box<libc::statx>,
    state: Arc<OpState<u64>>,
}
impl FileLenRequest {
    /// Creates a length query for `fd`, completing into `state`.
    pub fn new(fd: RawFd, state: Arc<OpState<u64>>) -> Self {
        Self {
            fd,
            statx: Box::new(unsafe { std::mem::zeroed() }),
            state,
        }
    }

    /// Returns the file descriptor to query.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Returns the statx buffer the kernel writes into; must stay alive
    /// until the CQE arrives.
    pub fn statx_mut_ptr(&mut self) -> *mut libc::statx {
        self.statx.as_mut()
    }

    /// Delivers the completion result to the waiting task.
    pub fn finish(self, result: RS<u64>) {
        complete_op(self.state, result);
    }
}
