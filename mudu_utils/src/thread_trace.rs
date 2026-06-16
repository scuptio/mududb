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

pub fn this_thread_id() -> TaskID {
    THREAD_CONTEXT.with(ThreadContextGuard::id)
}

pub struct ThreadTrace {
    watch: HashSet<String>,
}

pub struct NoopThreadTrace;

impl Default for NoopThreadTrace {
    fn default() -> Self {
        Self::new()
    }
}

impl NoopThreadTrace {
    pub fn new() -> Self {
        Self
    }

    pub fn watch(&self, _key: &str, _value: &str) {}
}

impl Default for ThreadTrace {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadTrace {
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

    pub fn backtrace() -> String {
        let id = this_thread_id();
        match TaskContext::get(id) {
            Some(ctx) => ctx.backtrace(),
            None => String::new(),
        }
    }

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

#[macro_export]
macro_rules! scoped_thread_trace {
    () => {
        let _thread_trace = $crate::thread_trace!();
    };
}

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

#[macro_export]
macro_rules! this_thread_id {
    () => {{ $crate::thread_trace::this_thread_id() }};
}
