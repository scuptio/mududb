use super::*;

/// Request to open a file via openat on the ring.
pub struct FileOpenRequest {
    path: CString,
    flags: i32,
    mode: u32,
    state: Arc<OpState<RawFd>>,
}
impl FileOpenRequest {
    /// Creates an open request for `path` with open(2) `flags` and creation
    /// `mode`, completing with the new fd into `state`.
    pub fn new(path: CString, flags: i32, mode: u32, state: Arc<OpState<RawFd>>) -> Self {
        Self {
            path,
            flags,
            mode,
            state,
        }
    }

    /// Returns the path to open.
    pub fn path(&self) -> &CString {
        &self.path
    }

    /// Returns the open(2) flags.
    pub fn flags(&self) -> i32 {
        self.flags
    }

    /// Returns the creation mode bits.
    pub fn mode(&self) -> u32 {
        self.mode
    }

    /// Delivers the completion result (the new fd) to the waiting task.
    pub fn finish(self, result: RS<RawFd>) {
        complete_op(self.state, result);
    }
}
