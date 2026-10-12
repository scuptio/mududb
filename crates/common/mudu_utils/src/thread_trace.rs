use crate::task_context::TaskContext;
use crate::task_id::{TaskID, new_task_id};
use scc::HashSet;
use std::backtrace::Backtrace;

struct ThreadContextGuard {
    id: TaskID,
    owned: bool,
}

impl ThreadContextGuard {
    fn new() -> Self {
        if let Some(id) = mudu_sys::task::sync::try_this_thread_task_id() {
            return Self { id, owned: false };
        }
        let id = new_task_id();
        let name = std::thread::current()
            .name()
            .map(|name| name.to_string())
            .unwrap_or_else(|| format!("thread-{id}"));

        let _ = TaskContext::new(id, name);
        Self { id, owned: true }
    }

    fn id(&self) -> TaskID {
        self.id
    }
}

impl Drop for ThreadContextGuard {
    fn drop(&mut self) {
        if self.owned {
            TaskContext::remove_context(self.id);
        }
    }
}

thread_local! {
    static THREAD_CONTEXT: ThreadContextGuard = ThreadContextGuard::new();
}

/// Returns the task ID of the current thread, registering a task context for
/// the thread on first use.
pub fn this_thread_id() -> TaskID {
    THREAD_CONTEXT.with(ThreadContextGuard::id)
}

/// Scoped trace guard for the current thread.
///
/// Constructing the guard captures the current thread backtrace and pushes it
/// onto the thread's task context; dropping it pops the frame and removes
/// every key recorded through [`ThreadTrace::watch`]. Instances are normally
/// created by the `thread_trace!` macro with the `debug_trace` feature
/// enabled.
pub struct ThreadTrace {
    watch: HashSet<String>,
}

/// No-op counterpart of [`ThreadTrace`] used when the `debug_trace` feature
/// is disabled.
pub struct NoopThreadTrace;

impl Default for NoopThreadTrace {
    fn default() -> Self {
        Self::new()
    }
}

impl NoopThreadTrace {
    /// Creates a no-op trace guard.
    pub fn new() -> Self {
        Self
    }

    /// Does nothing.
    pub fn watch(&self, _key: &str, _value: &str) {}
}

impl Default for ThreadTrace {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadTrace {
    /// Creates a trace guard and pushes the current thread backtrace onto the
    /// thread's task context.
    pub fn new() -> Self {
        Self::enter();
        Self {
            watch: HashSet::new(),
        }
    }

    fn enter() {
        let id = this_thread_id();
        if let Some(ctx) = TaskContext::get(id) {
            ctx.enter_thread(Backtrace::force_capture().to_string());
        }
    }

    /// Records the `key`/`value` pair on the current thread's task context.
    ///
    /// The key is remembered by this guard and removed from the context when
    /// the guard is dropped. Does nothing if the thread has no context.
    pub fn watch(&self, key: &str, value: &str) {
        let id = this_thread_id();
        if let Some(ctx) = TaskContext::get(id) {
            ctx.watch(key, value);
            let _ = self.watch.insert_sync(key.to_string());
        }
    }

    fn unwatch_all(&self) {
        let id = this_thread_id();
        if let Some(ctx) = TaskContext::get(id) {
            self.watch.iter_sync(|key| {
                ctx.unwatch(key);
                true
            });
        }
        self.watch.clear_sync();
    }

    fn exit() {
        let id = this_thread_id();
        if let Some(ctx) = TaskContext::get(id) {
            ctx.exit_thread();
        }
    }

    /// Returns the recorded backtrace of the current thread, or an empty
    /// string if the thread has no context.
    pub fn backtrace() -> String {
        let id = this_thread_id();
        match TaskContext::get(id) {
            Some(ctx) => ctx.backtrace(),
            None => String::new(),
        }
    }

    /// Returns a dump of the traces of all registered task contexts.
    pub fn dump_thread_trace() -> String {
        TaskContext::dump_task_trace()
    }
}

impl Drop for ThreadTrace {
    fn drop(&mut self) {
        Self::exit();
        self.unwatch_all();
    }
}

/// Creates a thread trace guard for the current scope.
///
/// Expands to a [`ThreadTrace`](crate::thread_trace::ThreadTrace) with the
/// `debug_trace` feature enabled, and to a
/// [`NoopThreadTrace`](crate::thread_trace::NoopThreadTrace) otherwise.
#[macro_export]
macro_rules! thread_trace {
    () => {{
        #[cfg(feature = "debug_trace")]
        {
            $crate::thread_trace::ThreadTrace::new()
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            $crate::thread_trace::NoopThreadTrace::new()
        }
    }};
}

/// Creates an anonymous thread trace guard that lives until the end of the
/// current scope.
#[macro_export]
macro_rules! scoped_thread_trace {
    () => {
        let _thread_trace = $crate::thread_trace!();
    };
}

/// Returns a dump of the traces of all registered task contexts.
///
/// Expands to an empty string when the `debug_trace` feature is disabled.
#[macro_export]
macro_rules! dump_thread_trace {
    () => {{
        #[cfg(feature = "debug_trace")]
        {
            $crate::thread_trace::ThreadTrace::dump_thread_trace()
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            String::new()
        }
    }};
}

/// Returns the recorded backtrace of the current thread.
///
/// Expands to an empty string when the `debug_trace` feature is disabled.
#[macro_export]
macro_rules! thread_backtrace {
    () => {{
        #[cfg(feature = "debug_trace")]
        {
            $crate::thread_trace::ThreadTrace::backtrace()
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            String::new()
        }
    }};
}

/// Returns the task ID of the current thread.
#[macro_export]
macro_rules! this_thread_id {
    () => {{ $crate::thread_trace::this_thread_id() }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_thread_id_returns_valid_id() {
        let id = this_thread_id();
        assert_eq!(id, this_thread_id());
        assert_eq!(id, this_thread_id!());
    }

    #[test]
    fn thread_trace_lifecycle_does_not_panic() {
        let trace = ThreadTrace::new();
        trace.watch("key", "value");
        let bt = ThreadTrace::backtrace();
        assert!(bt.is_empty() || bt.contains("thread backtrace"));
        let dump = ThreadTrace::dump_thread_trace();
        let _ = dump;
        drop(trace);
    }

    #[test]
    fn thread_trace_macros_do_not_panic() {
        let _id = this_thread_id!();
        let tt = thread_trace!();
        tt.watch("macro-key", "macro-value");
        let _bt = thread_backtrace!();
        let _dump = dump_thread_trace!();
        {
            scoped_thread_trace!();
        }
    }

    #[test]
    fn noop_thread_trace_is_noop() {
        let noop = NoopThreadTrace::new();
        noop.watch("k", "v");
    }
}
