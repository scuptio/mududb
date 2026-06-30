use super::*;

#[cfg(target_os = "linux")]
pub(crate) struct FileLenFuture {
    fd: RawFd,
    state: FileFutureState<u64>,
}

#[cfg(target_os = "linux")]
impl FileLenFuture {
    pub fn new(fd: RawFd) -> Self {
        Self {
            fd,
            state: FileFutureState::Init,
        }
    }
}

#[cfg(target_os = "linux")]
impl FileIoFuture<u64> for FileLenFuture {
    fn state(&mut self) -> &mut FileFutureState<u64> {
        &mut self.state
    }

    fn register(&mut self, state: Arc<OpState<u64>>) -> RS<WorkerRingOp> {
        Ok(WorkerRingOp::File(FileIoRequest::Len(FileLenRequest::new(
            self.fd, state,
        ))))
    }
}

#[cfg(target_os = "linux")]
impl Future for FileLenFuture {
    type Output = RS<u64>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        poll_file_io_future(self.get_mut(), cx)
    }
}
