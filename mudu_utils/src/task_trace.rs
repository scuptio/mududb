use async_backtrace::Location as BtLoc;
use scc::HashSet;

use crate::task_context::TaskContext;

pub use crate::task::this_task_id;

pub struct TaskTrace {
    watch: HashSet<String>,
}

pub struct NoopTaskTrace;

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
        let _id = this_task_id();
        let opt = TaskContext::get(_id);
        if let Some(_t) = opt {
            _t.enter(location);
        }
    }

    pub fn watch(&self, key: &str, value: &str) {
        let _id = this_task_id();
        let opt = TaskContext::get(_id);
        if let Some(_t) = opt {
            _t.watch(key, value);
            let _ = self.watch.insert_sync(key.to_string());
        }
    }

    fn unwatch_all(&self) {
        let _id = this_task_id();
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
        let _id = this_task_id();
        let opt = TaskContext::get(_id);
        if let Some(_t) = opt {
            _t.exit();
        }
    }

    pub fn backtrace() -> String {
        let _id = this_task_id();
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

impl Drop for TaskTrace {
    fn drop(&mut self) {
        TaskTrace::exit();
        self.unwatch_all();
    }
}
