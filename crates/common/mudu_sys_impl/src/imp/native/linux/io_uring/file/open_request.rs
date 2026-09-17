use super::*;

pub struct FileOpenRequest {
    path: CString,
    flags: i32,
    mode: u32,
    state: Arc<OpState<RawFd>>,
}
impl FileOpenRequest {
    pub fn new(path: CString, flags: i32, mode: u32, state: Arc<OpState<RawFd>>) -> Self {
        Self {
            path,
            flags,
            mode,
            state,
        }
    }

    pub fn path(&self) -> &CString {
        &self.path
    }

    pub fn flags(&self) -> i32 {
        self.flags
    }

    pub fn mode(&self) -> u32 {
        self.mode
    }

    pub fn finish(self, result: RS<RawFd>) {
        complete_op(self.state, result);
    }
}
