use super::*;

pub struct FileCloseRequest {
    fd: RawFd,
    state: Arc<OpState<()>>,
}
impl FileCloseRequest {
    pub fn new(fd: RawFd, state: Arc<OpState<()>>) -> Self {
        Self { fd, state }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn finish(self, result: RS<()>) {
        complete_op(self.state, result);
    }
}
