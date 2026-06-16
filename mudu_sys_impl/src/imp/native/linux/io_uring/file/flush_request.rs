use super::*;

pub struct FileFlushRequest {
    fd: RawFd,
    payload: Option<Box<dyn Any + Send>>,
    state: Arc<OpState<Box<dyn Any + Send>>>,
}
impl FileFlushRequest {
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

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    fn finish_boxed(self, result: RS<Box<dyn Any + Send>>) {
        complete_op(self.state, result);
    }

    pub fn finish_success(mut self) {
        let payload = self
            .payload
            .take()
            .expect("flush payload must be present when completing");
        self.finish_boxed(Ok(payload));
    }

    pub fn finish_error(self, err: mudu::error::err::MError) {
        self.finish_boxed(Err(err));
    }
}
