use async_backtrace::Location as BtLoc;
use lazy_static::lazy_static;
use scc::HashIndex;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use super::id::TaskID;

lazy_static! {
    static ref TASK_CONTEXT: HashIndex<TaskID, Arc<TaskContext>> = HashIndex::new();
}

/// Per-task tracing context registered in a process-wide registry keyed by
/// [`TaskID`].
///
/// Records the task name, its kind, a stack of async source locations, a
/// stack of thread backtraces, and arbitrary watched key/value pairs; all of
/// it is rendered by [`TaskContext::backtrace`].
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

/// Which execution environment a task context belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskContextKind {
    /// A plain OS thread spawned through this crate.
    Thread,
    /// A Tokio task.
    TokioTask,
    /// A task driven by the io_uring worker loop.
    IoUringTask,
}

impl TaskContextKind {
    /// Returns the lowercase string form used in trace output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Thread => "thread",
            Self::TokioTask => "tokio-task",
            Self::IoUringTask => "iouring-task",
        }
    }
}

impl TaskContext {
    /// Registers a new thread-kind context with the given id and name.
    pub fn new(id: TaskID, name: String) -> Arc<Self> {
        Self::new_context_with_kind(id, name, TaskContextKind::Thread)
    }

    /// Registers a new context of the given kind with the given id and name.
    pub fn new_context_with_kind(id: TaskID, name: String, kind: TaskContextKind) -> Arc<Self> {
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

    /// Registers a new thread-kind context.
    pub fn new_thread_context(id: TaskID, name: String) -> Arc<Self> {
        Self::new_context_with_kind(id, name, TaskContextKind::Thread)
    }

    /// Registers a new Tokio-task-kind context.
    pub fn new_tokio_context(id: TaskID, name: String) -> Arc<Self> {
        Self::new_context_with_kind(id, name, TaskContextKind::TokioTask)
    }

    /// Registers a new io_uring-task-kind context.
    pub fn new_iouring_context(id: TaskID, name: String) -> Arc<Self> {
        Self::new_context_with_kind(id, name, TaskContextKind::IoUringTask)
    }

    /// Removes the context with the given id from the registry.
    pub fn remove_context(id: TaskID) {
        let _ = TASK_CONTEXT.remove_sync(&id);
    }

    /// Looks up the context registered under `id`.
    pub fn get(id: TaskID) -> Option<Arc<TaskContext>> {
        TASK_CONTEXT.get_sync(&id).map(|entry| entry.get().clone())
    }

    /// Returns this context's task id.
    pub fn id(&self) -> TaskID {
        self.id
    }

    /// Records or replaces a watched key/value pair.
    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in task context ownership"
    )]
    pub fn watch(&self, k: &str, v: &str) {
        let mut watch_data = self.watch_data.lock().unwrap();
        let _ = watch_data.insert(k.to_string(), v.to_string());
    }

    /// Removes the watched pair with key `k`.
    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in task context ownership"
    )]
    pub fn unwatch(&self, k: &str) {
        let mut watch_data = self.watch_data.lock().unwrap();
        let _ = watch_data.remove(k);
    }

    /// Returns the task name.
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Returns the task kind.
    pub fn kind(&self) -> TaskContextKind {
        self.kind
    }

    /// Pushes an async source location onto the backtrace stack.
    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in task context ownership"
    )]
    pub fn enter(&self, loc: BtLoc) {
        let mut backtrace = self.backtrace.lock().unwrap();
        backtrace.push_back(loc);
    }

    /// Pops the most recently entered async source location.
    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in task context ownership"
    )]
    pub fn exit(&self) {
        let mut backtrace = self.backtrace.lock().unwrap();
        let _ = backtrace.pop_back();
    }

    /// Pushes a thread backtrace (rendered text) onto the thread backtrace
    /// stack.
    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in task context ownership"
    )]
    pub fn enter_thread(&self, trace: String) {
        let mut backtrace = self.thread_backtrace.lock().unwrap();
        backtrace.push_back(trace);
    }

    /// Pops the most recently entered thread backtrace.
    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in task context ownership"
    )]
    pub fn exit_thread(&self) {
        let mut backtrace = self.thread_backtrace.lock().unwrap();
        let _ = backtrace.pop_back();
    }

    /// Renders the async backtrace, thread backtrace, and watch data as
    /// text.
    pub fn backtrace(&self) -> String {
        let mut out = String::new();
        self.push_async_backtrace(&mut out);
        self.push_thread_backtrace(&mut out);
        self.push_watch_data(&mut out);
        out
    }

    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in task context ownership"
    )]
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

    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in task context ownership"
    )]
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

    #[expect(
        clippy::unwrap_used,
        reason = "mutex poisoning indicates a logic bug in task context ownership"
    )]
    fn push_watch_data(&self, out: &mut String) {
        let watch_data = self.watch_data.lock().unwrap();
        if !watch_data.is_empty() {
            out.push_str("watch:\n");
        }
        for (k, v) in watch_data.iter() {
            out.push_str(format!("=== {}:\t=\t{}\n", k, v).as_str());
        }
    }

    /// Renders the contexts of all registered tasks (kind, name, id, and
    /// backtrace) as text.
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{TaskContext, TaskContextKind};

    #[test]
    fn insert_get_remove() {
        let id = 12345_u128;
        let ctx = TaskContext::new(id, "test".to_string());
        assert_eq!(ctx.id(), id);

        let found = TaskContext::get(id).unwrap();
        assert_eq!(found.id(), id);
        assert_eq!(found.name(), "test");

        TaskContext::remove_context(id);
        assert!(TaskContext::get(id).is_none());
    }

    #[test]
    fn get_missing_returns_none() {
        let id = 99999_u128;
        TaskContext::remove_context(id);
        assert!(TaskContext::get(id).is_none());
    }

    #[test]
    fn watch_unwatch() {
        let id = 11111_u128;
        let ctx = TaskContext::new(id, "watch-test".to_string());
        ctx.watch("key", "value");
        let trace = ctx.backtrace();
        assert!(trace.contains("key"));
        assert!(trace.contains("value"));

        ctx.unwatch("key");
        let trace = ctx.backtrace();
        assert!(!trace.contains("key"));

        TaskContext::remove_context(id);
    }

    #[test]
    fn kind_default() {
        let id = 22222_u128;
        let ctx = TaskContext::new(id, "kind-test".to_string());
        assert_eq!(ctx.kind(), TaskContextKind::Thread);
        assert_eq!(ctx.kind().as_str(), "thread");
        TaskContext::remove_context(id);
    }
}
