use mudu::common::result::RS;
#[cfg(target_os = "linux")]
use mudu::error::ErrorCode;
#[cfg(target_os = "linux")]
use mudu::error::MuduError;
#[cfg(target_os = "linux")]
use mudu::mudu_error;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

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

pub struct OpState<T> {
    inner: Mutex<OpStateInner<T>>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn op_state<T>() -> Arc<OpState<T>> {
    Arc::new(OpState {
        inner: Mutex::new(OpStateInner {
            result: None,
            waker: None,
        }),
    })
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn complete_op<T>(state: Arc<OpState<T>>, result: RS<T>) {
    if let Ok(mut inner) = state.inner.lock() {
        inner.result = Some(result);
        if let Some(waker) = inner.waker.take() {
            waker.wake();
        }
    }
}

pub fn poll_op<T>(state: &Arc<OpState<T>>, cx: &mut Context<'_>) -> Poll<RS<T>> {
    if let Ok(mut inner) = state.inner.lock() {
        if let Some(result) = inner.result.take() {
            return Poll::Ready(result);
        }
        inner.waker = Some(cx.waker().clone());
    }
    Poll::Pending
}

pub fn try_take_op<T>(state: &Arc<OpState<T>>) -> Option<RS<T>> {
    state.inner.lock().ok()?.result.take()
}
