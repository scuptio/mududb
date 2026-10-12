use super::*;

/// Handle to a submitted fsync; awaiting it yields the caller-provided
/// payload once the flush completes.
#[derive(Clone)]
pub struct FlushHandle<P> {
    /// Shared completion state carrying the type-erased payload.
    pub state: Arc<OpState<Box<dyn Any + Send>>>,
    /// Marker tying the handle to the payload type.
    pub _marker: std::marker::PhantomData<P>,
}
impl<P> FlushHandle<P>
where
    P: Send + 'static,
{
    /// Waits for the flush to complete and returns the payload.
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
