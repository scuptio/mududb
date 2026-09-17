use async_backtrace::Location as BtLoc;
use scc::HashSet;

use super::context::TaskContext;

pub use super::async_::this_task_id;
use super::async_::{current_poll_task_id, try_this_task_id};

pub struct TaskTrace {
    watch: HashSet<String>,
}

pub struct NoopTaskTrace;

impl Default for NoopTaskTrace {
    fn default() -> Self {
        Self::new()
    }
}

impl NoopTaskTrace {
    pub fn new() -> Self {
        Self
    }

    pub fn watch(&self, _key: &str, _value: &str) {}
}

impl TaskTrace {
    pub fn new(location: BtLoc) -> Self {
        Self::enter(location);
        Self {
            watch: HashSet::new(),
        }
    }

    fn enter(location: BtLoc) {
        let Some(_id) = current_debug_task_id() else {
            return;
        };
        let opt = TaskContext::get(_id);
        if let Some(_t) = opt {
            _t.enter(location);
        }
    }

    pub fn watch(&self, key: &str, value: &str) {
        let Some(_id) = current_debug_task_id() else {
            return;
        };
        let opt = TaskContext::get(_id);
        if let Some(_t) = opt {
            _t.watch(key, value);
            let _ = self.watch.insert_sync(key.to_string());
        }
    }

    fn unwatch_all(&self) {
        let Some(_id) = current_debug_task_id() else {
            return;
        };
        let opt = TaskContext::get(_id);
        if let Some(_t) = opt {
            self.watch.iter_sync(|key| {
                _t.unwatch(key);
                true
            });
        }
        self.watch.clear_sync()
    }

    fn exit() {
        let Some(_id) = current_debug_task_id() else {
            return;
        };
        let opt = TaskContext::get(_id);
        if let Some(_t) = opt {
            _t.exit();
        }
    }

    pub fn backtrace() -> String {
        let Some(_id) = current_debug_task_id() else {
            return "".to_string();
        };
        let opt = TaskContext::get(_id);
        match opt {
            Some(_t) => _t.backtrace(),
            _ => "".to_string(),
        }
    }

    pub fn dump_task_trace() -> String {
        TaskContext::dump_task_trace()
    }
}

fn current_debug_task_id() -> Option<super::id::TaskID> {
    try_this_task_id().or_else(current_poll_task_id)
}

impl Drop for TaskTrace {
    fn drop(&mut self) {
        TaskTrace::exit();
        self.unwatch_all();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::PollTaskIdGuard;

    #[test]
    fn noop_task_trace_does_not_panic() {
        let t1 = NoopTaskTrace::new();
        let t2 = NoopTaskTrace;
        t1.watch("k", "v");
        t2.watch("k", "v");
    }

    #[test]
    fn task_trace_no_context_does_not_panic_and_returns_empty() {
        let loc = async_backtrace::location!();
        let trace = TaskTrace::new(loc);
        trace.watch("a", "b");
        assert!(TaskTrace::backtrace().is_empty());
        // dump_task_trace scans the global registry; other tests may leave
        // contexts behind, so we only assert it does not panic.
        let _ = TaskTrace::dump_task_trace();
        drop(trace);
    }

    #[test]
    fn task_trace_watch_appears_in_backtrace() {
        let id = 42_u128;
        let _ctx = TaskContext::new(id, "trace-test".to_string());
        let guard = PollTaskIdGuard::enter(id);
        let loc = async_backtrace::location!();
        let trace = TaskTrace::new(loc);
        trace.watch("stage", "plan");
        let bt = TaskTrace::backtrace();
        assert!(bt.contains("stage"));
        assert!(bt.contains("plan"));
        drop(trace);
        drop(guard);
        TaskContext::remove_context(id);
    }

    #[test]
    fn task_trace_unwatch_on_drop() {
        let id = 43_u128;
        let _ctx = TaskContext::new(id, "trace-drop-test".to_string());
        let guard = PollTaskIdGuard::enter(id);
        let loc = async_backtrace::location!();
        let trace = TaskTrace::new(loc);
        trace.watch("stage", "plan");
        drop(trace);
        let bt = TaskTrace::backtrace();
        assert!(!bt.contains("stage"));
        assert!(!bt.contains("plan"));
        drop(guard);
        TaskContext::remove_context(id);
    }

    #[test]
    fn dump_task_trace_contains_context_and_watch() {
        let id = 44_u128;
        let _ctx = TaskContext::new(id, "dump-test".to_string());
        let guard = PollTaskIdGuard::enter(id);
        let loc = async_backtrace::location!();
        let trace = TaskTrace::new(loc);
        trace.watch("k", "v");
        let dump = TaskTrace::dump_task_trace();
        assert!(dump.contains("dump-test"));
        assert!(dump.contains(&id.to_string()));
        assert!(dump.contains("k"));
        assert!(dump.contains("v"));
        drop(trace);
        drop(guard);
        TaskContext::remove_context(id);
    }
}
