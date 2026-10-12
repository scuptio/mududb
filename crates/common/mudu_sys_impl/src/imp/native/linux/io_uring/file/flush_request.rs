use super::*;

/// Request to fsync a file descriptor via io_uring, carrying a
/// caller-provided payload that is handed back on completion.
pub struct FileFlushRequest {
    fd: RawFd,
    payload: Option<Box<dyn Any + Send>>,
    state: Arc<OpState<Box<dyn Any + Send>>>,
}
impl FileFlushRequest {
    /// Creates a flush request for `fd` with `payload`, completing into
    /// `state`.
    pub fn new<P>(fd: RawFd, payload: P, state: Arc<OpState<Box<dyn Any + Send>>>) -> Self
    where
        P: Send + 'static,
    {
        Self {
            fd,
            payload: Some(Box::new(payload)),
            state,
        }
    }

    /// Returns the file descriptor to flush.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    fn finish_boxed(self, result: RS<Box<dyn Any + Send>>) {
        complete_op(self.state, result);
    }

    /// Completes the request successfully, returning the payload to the
    /// waiting task.
    pub fn finish_success(mut self) {
        if let Some(payload) = self.payload.take() {
            self.finish_boxed(Ok(payload));
        }
    }

    /// Completes the request with `err`.
    pub fn finish_error(self, err: mudu::error::MuduError) {
        self.finish_boxed(Err(err));
    }
}
