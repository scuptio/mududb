use super::*;

#[cfg(target_os = "linux")]
pub(crate) trait FileIoFuture<T> {
    fn state(&mut self) -> &mut FileFutureState<T>;
    fn register(&mut self, state: Arc<OpState<T>>) -> RS<WorkerRingOp>;
}

#[cfg(target_os = "linux")]
pub(crate) fn poll_file_io_future<T, F: FileIoFuture<T>>(
    future: &mut F,
    cx: &mut Context<'_>,
) -> Poll<RS<T>> {
    loop {
        let current = std::mem::replace(future.state(), FileFutureState::Done);
        match current {
            FileFutureState::Init => {
                let state = op_state();
                match future.register(state.clone()) {
                    Ok(op) => {
                        if let Err(err) = with_current_ring(|ring| ring.register(op).map(|_| ())) {
                            *future.state() = FileFutureState::Done;
                            return Poll::Ready(Err(err));
                        }
                        *future.state() = FileFutureState::Pending(state);
                    }
                    Err(err) => {
                        *future.state() = FileFutureState::Done;
                        return Poll::Ready(Err(err));
                    }
                }
            }
            FileFutureState::Pending(state) => {
                return match poll_op(&state, cx) {
                    Poll::Ready(result) => {
                        *future.state() = FileFutureState::Done;
                        Poll::Ready(result)
                    }
                    Poll::Pending => {
                        *future.state() = FileFutureState::Pending(state);
                        Poll::Pending
                    }
                }
            }
            FileFutureState::Done => return Poll::Pending,
        }
    }
}
