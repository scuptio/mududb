use async_backtrace::Location as BtLoc;
use scc::HashSet;

use crate::task_context::TaskContext;

pub use crate::task_async::this_task_id;
use crate::task_async::{current_poll_task_id, try_this_task_id};

/// Scoped trace guard for the current async task.
///
/// Constructing the guard pushes a location frame onto the current task's
/// backtrace stack in the task context registry; dropping it pops the frame
/// and removes every key recorded through [`TaskTrace::watch`]. Instances are
/// normally created by the `task_trace!` macro with the `debug_trace` feature
/// enabled.
pub struct TaskTrace {
    watch: HashSet<String>,
}

/// No-op counterpart of [`TaskTrace`] used when the `debug_trace` feature is
/// disabled.
pub struct NoopTaskTrace;

impl Default for NoopTaskTrace {
    fn default() -> Self {
        Self::new()
    }
}

impl NoopTaskTrace {
    /// Creates a no-op trace guard.
    pub fn new() -> Self {
        Self
    }

    /// Does nothing.
    pub fn watch(&self, _key: &str, _value: &str) {}
}

impl TaskTrace {
    /// Creates a trace guard without entering a location frame.
    pub fn new_empty() -> Self {
        Self {
            watch: HashSet::new(),
        }
    }
    /// Creates a trace guard and enters a frame at `location` on the current
    /// task's backtrace stack.
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

    /// Records the `key`/`value` pair on the current task's context.
    ///
    /// The key is remembered by this guard and removed from the context when
    /// the guard is dropped. Does nothing if the current task has no context.
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

    /// Returns the recorded backtrace of the current task, or an empty string
    /// if the task has no context.
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

    /// Returns a dump of the traces of all registered task contexts.
    pub fn dump_task_trace() -> String {
        TaskContext::dump_task_trace()
    }
}

fn current_debug_task_id() -> Option<mudu_sys::TaskID> {
    try_this_task_id().or_else(current_poll_task_id)
}

impl Drop for TaskTrace {
    fn drop(&mut self) {
        TaskTrace::exit();
        self.unwatch_all();
    }
}
