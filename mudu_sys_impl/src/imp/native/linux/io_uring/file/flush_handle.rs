use super::*;

#[derive(Clone)]
pub struct FlushHandle<P> {
    pub state: Arc<OpState<Box<dyn Any + Send>>>,
    pub _marker: std::marker::PhantomData<P>,
}
impl<P> FlushHandle<P>
where
    P: Send + 'static,
{
    pub async fn wait(self) -> RS<P> {
        let trace = task_trace!();
        trace.watch("file_handle.stage", "flush_wait");
        let result = wait_op(&self.state).await.and_then(|payload| {
            payload
                .downcast::<P>()
                .map(|boxed| *boxed)
                .map_err(|_| mudu_error!(ErrorCode::Internal, "file flush payload type mismatch"))
        });
        trace.watch("file_handle.stage", "flush_done");
        result
    }
}
