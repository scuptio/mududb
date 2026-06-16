use async_backtrace::Location as BtLoc;
use lazy_static::lazy_static;
use scc::HashIndex;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::task::id::TaskID;

lazy_static! {
    static ref TASK_CONTEXT: HashIndex<TaskID, Arc<TaskContext>> = HashIndex::new();
}

pub struct TaskContext {
    name: String,
    kind: TaskContextKind,
    id: u128,
    backtrace: Mutex<VecDeque<BtLoc>>,
    thread_backtrace: Mutex<VecDeque<String>>,
    // Keep watch_data on a plain Mutex<HashMap> instead of scc::HashMap.
    // This field is only for debug/trace metadata, and using scc here hit a
    // reclamation edge case during TLS/global destructor paths where drop
    // could stall inside the collector.
    watch_data: Mutex<HashMap<String, String>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskContextKind {
    Thread,
    TokioTask,
    IoUringTask,
}

impl TaskContextKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Thread => "thread",
            Self::TokioTask => "tokio-task",
            Self::IoUringTask => "iouring-task",
        }
    }
}

impl TaskContext {
    pub fn new(id: TaskID, name: String) -> Arc<Self> {
        Self::new_context_with_kind(id, name, TaskContextKind::Thread)
    }

    pub fn new_context(id: TaskID, name: String, local_task: bool) -> Arc<Self> {
        let kind = if local_task {
            TaskContextKind::Thread
        } else {
            TaskContextKind::TokioTask
        };
        Self::new_context_with_kind(id, name, kind)
    }

    pub fn new_context_with_kind(
        id: TaskID,
        name: String,
        kind: TaskContextKind,
    ) -> Arc<Self> {
        let ctx = Arc::new(Self {
            name,
            kind,
            id,
            backtrace: Default::default(),
            thread_backtrace: Default::default(),
            watch_data: Default::default(),
        });
        let _ = TASK_CONTEXT.insert_sync(ctx.id(), ctx.clone());
        ctx
    }

    pub fn new_thread_context(id: TaskID, name: String) -> Arc<Self> {
        Self::new_context_with_kind(id, name, TaskContextKind::Thread)
    }

    pub fn new_tokio_context(id: TaskID, name: String) -> Arc<Self> {
        Self::new_context_with_kind(id, name, TaskContextKind::TokioTask)
    }

    pub fn new_iouring_context(id: TaskID, name: String) -> Arc<Self> {
        Self::new_context_with_kind(id, name, TaskContextKind::IoUringTask)
    }

    pub fn remove_context(id: TaskID) {
        let _ = TASK_CONTEXT.remove_sync(&id);
    }

    pub fn get(id: TaskID) -> Option<Arc<TaskContext>> {
        TASK_CONTEXT.get_sync(&id).map(|entry| entry.get().clone())
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

    pub fn kind(&self) -> TaskContextKind {
        self.kind
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
        let mut out = String::new();
        self.push_async_backtrace(&mut out);
        self.push_thread_backtrace(&mut out);
        self.push_watch_data(&mut out);
        out
    }

    fn push_async_backtrace(&self, out: &mut String) {
        let deque = self.backtrace.lock().unwrap();
        if deque.is_empty() {
            return;
        }
        out.push_str("async backtrace:\n");
        for (depth, loc) in deque.iter().enumerate() {
            out.push_str("  ");
            for _ in 0..depth {
                out.push_str("--");
            }
            out.push_str("->");
            out.push_str(loc.to_string().as_str());
            out.push('\n');
        }
    }

    fn push_thread_backtrace(&self, out: &mut String) {
        let deque = self.thread_backtrace.lock().unwrap();
        if deque.is_empty() {
            return;
        }
        out.push_str("thread backtrace:\n");
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
    }

    fn push_watch_data(&self, out: &mut String) {
        let watch_data = self.watch_data.lock().unwrap();
        if !watch_data.is_empty() {
            out.push_str("watch:\n");
        }
        for (k, v) in watch_data.iter() {
            out.push_str(format!("=== {}:\t=\t{}\n", k, v).as_str());
        }
    }

    pub fn dump_task_trace() -> String {
        let mut out = String::new();
        let guard = scc::Guard::new();
        for (id, task) in TASK_CONTEXT.iter(&guard) {
            out.push_str(
                format!(
                    "kind:{},\t name:{},\t id: {},\t trace {}\n",
                    task.kind().as_str(),
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
