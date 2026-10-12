use super::*;

/// Request to close a file descriptor via io_uring.
pub struct FileCloseRequest {
    fd: RawFd,
    state: Arc<OpState<()>>,
}
impl FileCloseRequest {
    /// Creates a close request for `fd` completing into `state`.
    pub fn new(fd: RawFd, state: Arc<OpState<()>>) -> Self {
        Self { fd, state }
    }

    /// Returns the file descriptor to close.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Delivers the completion result to the waiting task.
    pub fn finish(self, result: RS<()>) {
        complete_op(self.state, result);
    }
}
