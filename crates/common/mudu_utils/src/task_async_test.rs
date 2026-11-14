//! Tests for the `run_async` helper.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use crate::task_async::run_async;

#[test]
fn run_async_executes_the_future_on_a_dedicated_thread() {
    let caller = std::thread::current().id();
    let worker = run_async(async { std::thread::current().id() }).unwrap();
    assert_ne!(caller, worker);
}

#[test]
fn run_async_returns_the_future_output() {
    assert_eq!(run_async(async { 40 + 2 }).unwrap(), 42);
}
