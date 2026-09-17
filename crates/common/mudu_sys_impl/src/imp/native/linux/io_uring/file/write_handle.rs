use super::*;

pub struct WriteHandle {
    pub state: Arc<OpState<usize>>,
}
impl WriteHandle {
    pub async fn wait(self) -> RS<usize> {
        scoped_task_trace!();
        wait_op(&self.state).await
    }
}
