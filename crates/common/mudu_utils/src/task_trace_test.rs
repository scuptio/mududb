//! Tests for `TaskTrace`, `NoopTaskTrace`, and the related backtrace helpers.
#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use crate::task_context::TaskContext;
use crate::task_trace::{NoopTaskTrace, TaskTrace};
use mudu_sys::task::async_::PollTaskIdGuard;

#[test]
fn noop_task_trace_can_watch_without_context() {
    let trace = NoopTaskTrace::new();
    trace.watch("key", "value");
    trace.watch("other", "data");
}

#[test]
fn task_trace_with_context_records_watch_and_backtrace() {
    let id = 4242_u128;
    let _guard = PollTaskIdGuard::enter(id);
    let _ctx = TaskContext::new(id, "task-trace-test".to_string());

    let loc = async_backtrace::location!();
    let trace = TaskTrace::new(loc);
    trace.watch("user", "alice");

    let backtrace = TaskTrace::backtrace();
    assert!(backtrace.contains("user"));
    assert!(backtrace.contains("alice"));

    let dump = TaskTrace::dump_task_trace();
    assert!(dump.contains("task-trace-test"));

    drop(trace);
    TaskContext::remove_context(id);
}

#[test]
fn task_trace_without_context_is_inert() {
    let loc = async_backtrace::location!();
    let trace = TaskTrace::new(loc);
    trace.watch("ignored", "value");

    assert!(TaskTrace::backtrace().is_empty());
    drop(trace);
}

#[test]
fn task_trace_drop_unwatches_all() {
    let id = 4343_u128;
    let _guard = PollTaskIdGuard::enter(id);
    let _ctx = TaskContext::new(id, "task-trace-drop-test".to_string());

    let loc = async_backtrace::location!();
    {
        let trace = TaskTrace::new(loc);
        trace.watch("temp", "data");
        assert!(TaskTrace::backtrace().contains("temp"));
        drop(trace);
    }

    // After the trace is dropped, the watched key should have been removed.
    let after_drop = TaskTrace::backtrace();
    assert!(!after_drop.contains("temp"));

    TaskContext::remove_context(id);
}
