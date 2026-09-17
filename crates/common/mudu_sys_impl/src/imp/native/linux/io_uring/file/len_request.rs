use super::*;

pub struct FileLenRequest {
    fd: RawFd,
    pub(crate) statx: Box<libc::statx>,
    state: Arc<OpState<u64>>,
}
impl FileLenRequest {
    pub fn new(fd: RawFd, state: Arc<OpState<u64>>) -> Self {
        Self {
            fd,
            statx: Box::new(unsafe { std::mem::zeroed() }),
            state,
        }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn statx_mut_ptr(&mut self) -> *mut libc::statx {
        self.statx.as_mut()
    }

    pub fn finish(self, result: RS<u64>) {
        complete_op(self.state, result);
    }
}
