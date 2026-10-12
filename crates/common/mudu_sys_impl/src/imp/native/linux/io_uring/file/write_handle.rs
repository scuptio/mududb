use super::*;

/// Handle to a submitted write; awaiting it yields the number of bytes
/// written.
pub struct WriteHandle {
    /// Shared completion state carrying the written byte count.
    pub state: Arc<OpState<usize>>,
}
impl WriteHandle {
    /// Waits for the write to complete and returns the byte count.
    pub async fn wait(self) -> RS<usize> {
        scoped_task_trace!();
        wait_op(&self.state).await
    }
}
