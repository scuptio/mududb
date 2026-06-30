use super::*;

#[cfg(target_os = "linux")]
pub(crate) struct FileCloseFuture {
    fd: RawFd,
    state: FileFutureState<()>,
}

#[cfg(target_os = "linux")]
impl FileCloseFuture {
    pub fn new(fd: RawFd) -> Self {
        Self {
            fd,
            state: FileFutureState::Init,
        }
    }
}

#[cfg(target_os = "linux")]
impl FileIoFuture<()> for FileCloseFuture {
    fn state(&mut self) -> &mut FileFutureState<()> {
        &mut self.state
    }

    fn register(&mut self, state: Arc<OpState<()>>) -> RS<WorkerRingOp> {
        Ok(WorkerRingOp::File(FileIoRequest::Close(
            FileCloseRequest::new(self.fd, state),
        )))
    }
}

#[cfg(target_os = "linux")]
impl Future for FileCloseFuture {
    type Output = RS<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        poll_file_io_future(self.get_mut(), cx)
    }
}
