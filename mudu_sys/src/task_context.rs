use async_backtrace::Location as BtLoc;
use lazy_static::lazy_static;
use scc::HashIndex;
use std::collections::{HashMap, VecDeque};
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
    // Keep watch_data on a plain Mutex<HashMap> instead of scc::HashMap.
    // This field is only for debug/trace metadata, and using scc here hit a
    // reclamation edge case during TLS/global destructor paths where drop
    // could stall inside the collector.
    watch_data: Mutex<HashMap<String, String>>,
}

impl TaskContext {
    pub fn new(id: TaskID, name: String) -> Arc<Self> {
        Self::new_context(id, name, true)
    }

    pub fn new_context(id: TaskID, name: String, local_task: bool) -> Arc<Self> {
        let ctx = Arc::new(Self {
            name,
            local_task,
            id,
            backtrace: Default::default(),
            thread_backtrace: Default::default(),
            watch_data: Default::default(),
        });
        let _ = TASK_CONTEXT.insert_sync(ctx.id(), ctx.clone());
        ctx
    }

    pub fn remove_context(id: TaskID) {
        let _ = TASK_CONTEXT.remove_sync(&id);
    }

    pub fn get(id: TaskID) -> Option<Arc<TaskContext>> {
        TASK_CONTEXT.get_sync(&id).map(|entry| entry.get().clone())
    }

    pub fn is_local(&self) -> bool {
        self.local_task
    }

    pub fn id(&self) -> TaskID {
        self.id
    }

    pub fn watch(&self, k: &str, v: &str) {
        let mut watch_data = self.watch_data.lock().unwrap();
        let _ = watch_data.insert(k.to_string(), v.to_string());
    }

    pub fn unwatch(&self, k: &str) {
        let mut watch_data = self.watch_data.lock().unwrap();
        let _ = watch_data.remove(k);
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn enter(&self, loc: BtLoc) {
        let mut backtrace = self.backtrace.lock().unwrap();
        backtrace.push_back(loc);
    }

    pub fn exit(&self) {
        let mut backtrace = self.backtrace.lock().unwrap();
        let _ = backtrace.pop_back();
    }

    pub fn enter_thread(&self, trace: String) {
        let mut backtrace = self.thread_backtrace.lock().unwrap();
        backtrace.push_back(trace);
    }

    pub fn exit_thread(&self) {
        let mut backtrace = self.thread_backtrace.lock().unwrap();
        let _ = backtrace.pop_back();
    }

    pub fn backtrace(&self) -> String {
        if self.local_task {
            return self.thread_backtrace_string();
        }
        self.task_backtrace_string()
    }

    fn task_backtrace_string(&self) -> String {
        let deque = self.backtrace.lock().unwrap();
        let mut out = String::from("backtrace:\n");
        for (depth, loc) in deque.iter().enumerate() {
            out.push_str("  ");
            for _ in 0..depth {
                out.push_str("--");
            }
            out.push_str("->");
            out.push_str(loc.to_string().as_str());
            out.push('\n');
        }
        let watch_data = self.watch_data.lock().unwrap();
        if !watch_data.is_empty() {
            out.push_str("watch:\n");
        }
        for (k, v) in watch_data.iter() {
            out.push_str(format!("=== {}:\t=\t{}\n", k, v).as_str());
        }
        out
    }

    fn thread_backtrace_string(&self) -> String {
        let deque = self.thread_backtrace.lock().unwrap();
        let mut out = String::from("backtrace:\n");
        for (depth, trace) in deque.iter().enumerate() {
            out.push_str("  ");
            for _ in 0..depth {
                out.push_str("--");
            }
            out.push_str("->");
            out.push_str(trace);
            if !trace.ends_with('\n') {
                out.push('\n');
            }
        }
        let watch_data = self.watch_data.lock().unwrap();
        if !watch_data.is_empty() {
            out.push_str("watch:\n");
        }
        for (k, v) in watch_data.iter() {
            out.push_str(format!("=== {}:\t=\t{}\n", k, v).as_str());
        }
        out
    }

    pub fn dump_task_trace() -> String {
        let mut out = String::new();
        let guard = scc::Guard::new();
        for (id, task) in TASK_CONTEXT.iter(&guard) {
            out.push_str(
                format!(
                    "name:{},\t id: {},\t trace {}\n",
                    task.name(),
                    id,
                    task.backtrace()
                )
                .as_str(),
            );
        }
        out
    }
}
