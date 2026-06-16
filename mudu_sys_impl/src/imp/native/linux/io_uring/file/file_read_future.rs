use super::*;

#[cfg(target_os = "linux")]
pub(crate) struct FileReadFuture {
    fd: RawFd,
    len: usize,
    offset: u64,
    state: FileFutureState<Vec<u8>>,
}

#[cfg(target_os = "linux")]
impl FileReadFuture {
    pub fn new(fd: RawFd, len: usize, offset: u64) -> Self {
        Self {
            fd,
            len,
            offset,
            state: FileFutureState::Init,
        }
    }
}

#[cfg(target_os = "linux")]
impl FileIoFuture<Vec<u8>> for FileReadFuture {
    fn state(&mut self) -> &mut FileFutureState<Vec<u8>> {
        &mut self.state
    }

    fn register(&mut self, state: Arc<OpState<Vec<u8>>>) -> RS<WorkerRingOp> {
        Ok(WorkerRingOp::File(FileIoRequest::Read(
            FileReadRequest::new(self.fd, self.len, self.offset, state),
        )))
    }
}

#[cfg(target_os = "linux")]
impl Future for FileReadFuture {
    type Output = RS<Vec<u8>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        poll_file_io_future(self.get_mut(), cx)
    }
}
