use super::*;

#[cfg(target_os = "linux")]
pub(crate) struct FileOpenFuture {
    path: Option<CString>,
    flags: i32,
    mode: u32,
    state: FileFutureState<RawFd>,
}

#[cfg(target_os = "linux")]
impl FileOpenFuture {
    pub fn new(path: CString, flags: i32, mode: u32) -> Self {
        Self {
            path: Some(path),
            flags,
            mode,
            state: FileFutureState::Init,
        }
    }
}

#[cfg(target_os = "linux")]
impl FileIoFuture<RawFd> for FileOpenFuture {
    fn state(&mut self) -> &mut FileFutureState<RawFd> {
        &mut self.state
    }

    fn register(&mut self, state: Arc<OpState<RawFd>>) -> RS<WorkerRingOp> {
        let path = self.path.take().ok_or_else(|| {
            m_error!(EC::InternalErr, "file open future already registered")
        })?;
        tracing::debug!(path = %path.to_string_lossy(), "file open future register");
        Ok(WorkerRingOp::File(FileIoRequest::Open(
            FileOpenRequest::new(path, self.flags, self.mode, state),
        )))
    }
}

#[cfg(target_os = "linux")]
impl Future for FileOpenFuture {
    type Output = RS<RawFd>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match poll_file_io_future(self.get_mut(), cx) {
            ready @ Poll::Ready(Ok(_)) => {
                tracing::debug!("file open future ready");
                ready
            }
            other => other,
        }
    }
}
