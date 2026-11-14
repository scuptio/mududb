use mudu::common::result::RS;
#[cfg(target_os = "linux")]
use mudu::error::ErrorCode;
#[cfg(target_os = "linux")]
use mudu::error::MuduError;
#[cfg(target_os = "linux")]
use mudu::mudu_error;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

/// Builds an error for a failed io_uring user operation completion, mapping
/// the negative completion `result` to the matching OS error code (Linux
/// only).
#[cfg(target_os = "linux")]
pub fn completion_error(kind: &'static str, result: i32) -> MuduError {
    mudu_error!(
        ErrorCode::from_raw_os_error(-result),
        format!("worker user {} completion error {}", kind, result)
    )
}

struct OpStateInner<T> {
    result: Option<RS<T>>,
    waker: Option<Waker>,
}

/// Shared state of one in-flight IO operation: holds the completion result
/// and the waker of the task waiting for it.
pub struct OpState<T> {
    inner: Mutex<OpStateInner<T>>,
}

/// Creates a fresh, uncompleted operation state.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn op_state<T>() -> Arc<OpState<T>> {
    Arc::new(OpState {
        inner: Mutex::new(OpStateInner {
            result: None,
            waker: None,
        }),
    })
}

/// Stores the completion `result` in `state` and wakes the waiting task, if
/// one is registered.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn complete_op<T>(state: Arc<OpState<T>>, result: RS<T>) {
    if let Ok(mut inner) = state.inner.lock() {
        inner.result = Some(result);
        if let Some(waker) = inner.waker.take() {
            waker.wake();
        }
    }
}

/// Polls the operation state: returns the completion result if available,
/// otherwise registers the current task's waker and returns `Pending`.
pub fn poll_op<T>(state: &Arc<OpState<T>>, cx: &mut Context<'_>) -> Poll<RS<T>> {
    if let Ok(mut inner) = state.inner.lock() {
        if let Some(result) = inner.result.take() {
            return Poll::Ready(result);
        }
        inner.waker = Some(cx.waker().clone());
    }
    Poll::Pending
}

/// Takes the completion result out of `state` without registering a waker,
/// if the operation has completed.
pub fn try_take_op<T>(state: &Arc<OpState<T>>) -> Option<RS<T>> {
    state.inner.lock().ok()?.result.take()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    #[cfg(target_os = "linux")]
    #[test]
    fn completion_error_maps_negative_result_to_os_error() {
        let err = super::completion_error("read", -2);
        assert_eq!(err.ec(), mudu::error::ErrorCode::from_raw_os_error(2));
    }

    #[test]
    fn op_state_complete_then_try_take() {
        let state = super::op_state::<u32>();
        assert!(super::try_take_op(&state).is_none());
        super::complete_op(state.clone(), Ok(7));
        assert_eq!(super::try_take_op(&state).unwrap().unwrap(), 7);
        assert!(super::try_take_op(&state).is_none());
    }

    #[test]
    fn poll_op_registers_waker_and_resolves_after_complete() {
        let state = super::op_state::<u32>();
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        assert!(super::poll_op(&state, &mut cx).is_pending());
        super::complete_op(state.clone(), Ok(9));
        match super::poll_op(&state, &mut cx) {
            std::task::Poll::Ready(result) => assert_eq!(result.unwrap(), 9),
            std::task::Poll::Pending => panic!("operation must be completed"),
        }
    }
}
