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
