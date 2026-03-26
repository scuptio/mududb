use async_backtrace::Location as BtLoc;
use lazy_static::lazy_static;
use scc::{HashIndex, HashMap};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::task_id::TaskID;

lazy_static! {
    static ref TASK_CONTEXT: HashIndex<TaskID, Arc<TaskContext>> = HashIndex::new();
}

pub struct TaskContext {
    name: String,
    local_task: bool,
    id: u128,
    backtrace: Mutex<VecDeque<BtLoc>>,
    thread_backtrace: Mutex<VecDeque<String>>,
    watch_data: HashMap<String, String>,
}

impl TaskContext {
    pub fn new(id: TaskID, name: String) -> Arc<Self> {
        Self::new_context(id, name, true)
    }

    pub fn new_context(id: TaskID, name: String, local_task: bool) -> Arc<Self> {
        let r = Self {
            name,

            local_task,
            id,
            backtrace: Default::default(),
            thread_backtrace: Default::default(),
            watch_data: Default::default(),
        };
        let ret = Arc::new(r);
        let id = ret.id();
        let _ = TASK_CONTEXT.insert_sync(id, ret.clone());
        ret
    }

    pub fn remove_context(id: TaskID) {
        let _ = TASK_CONTEXT.remove_sync(&id);
    }

    pub fn get(id: TaskID) -> Option<Arc<TaskContext>> {
        let opt = TASK_CONTEXT.get_sync(&id);
        opt.map(|e| e.get().clone())
    }

    pub fn is_local(&self) -> bool {
        self.local_task
    }

    pub fn id(&self) -> TaskID {
        self.id
    }

    pub fn watch(&self, k: &str, v: &str) {
        let _ = self.watch_data.insert_sync(k.to_string(), v.to_string());
    }

    pub fn unwatch(&self, k: &str) {
        self.watch_data.remove_sync(k);
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn enter(&self, l: BtLoc) {
        let mut location = self.backtrace.lock().unwrap();
        location.push_back(l);
    }

    pub fn exit(&self) {
        let mut location = self.backtrace.lock().unwrap();
        let _ = location.pop_back();
    }

    pub fn enter_thread(&self, trace: String) {
        let mut location = self.thread_backtrace.lock().unwrap();
        location.push_back(trace);
    }

    pub fn exit_thread(&self) {
        let mut location = self.thread_backtrace.lock().unwrap();
        let _ = location.pop_back();
    }

    pub fn backtrace(&self) -> String {
        if self.local_task {
            return self.thread_backtrace_string();
        }
        self.task_backtrace_string()
    }

    fn task_backtrace_string(&self) -> String {
        let deque = self.backtrace.lock().unwrap();
        let mut s = String::new();
        s.push_str("backtrace:\n");
        for (n, l) in deque.iter().enumerate() {
            s.push_str("  ");
            for _ in 0..n {
                s.push_str("--");
            }
            s.push_str("->");
            s.push_str(l.to_string().as_str());
            s.push('\n');
        }
        if !self.watch_data.is_empty() {
            s.push_str("watch:\n");
        }
        self.watch_data.iter_sync(|k, v| {
            s.push_str(format!("=== {}:\t=\t{}\n", k, v).as_str());
            true
        });

        s
    }

    fn thread_backtrace_string(&self) -> String {
        let deque = self.thread_backtrace.lock().unwrap();
        let mut s = String::new();
        s.push_str("backtrace:\n");
        for (n, trace) in deque.iter().enumerate() {
            s.push_str("  ");
            for _ in 0..n {
                s.push_str("--");
            }
            s.push_str("->");
            s.push_str(trace);
            if !trace.ends_with('\n') {
                s.push('\n');
            }
        }
        if !self.watch_data.is_empty() {
            s.push_str("watch:\n");
        }
        self.watch_data.iter_sync(|k, v| {
            s.push_str(format!("=== {}:\t=\t{}\n", k, v).as_str());
            true
        });

        s
    }

    pub fn dump_task_trace() -> String {
        let mut ret = String::new();
        let guard = scc::Guard::new();
        for (_id, task) in TASK_CONTEXT.iter(&guard) {
            let s = format!(
                "name:{},\t id: {},\t trace {}\n",
                task.name(),
                _id,
                task.backtrace()
            );
            ret.push_str(s.as_str());
        }
        ret
    }
}
