use async_backtrace::Location as BtLoc;
use scc::HashSet;

use crate::task_context::TaskContext;

pub use crate::task_async::this_task_id;
use crate::task_async::{current_poll_task_id, try_this_task_id};

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
    pub fn new_empty() -> Self {
        Self {
            watch: HashSet::new(),
        }
    }
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

fn current_debug_task_id() -> Option<mudu_sys::TaskID> {
    try_this_task_id().or_else(current_poll_task_id)
}

impl Drop for TaskTrace {
    fn drop(&mut self) {
        TaskTrace::exit();
        self.unwatch_all();
    }
}
