/// Creates a trace guard for the current scope, capturing the invocation
/// location.
///
/// Expands to a [`TaskTrace`](crate::task_trace::TaskTrace) with the
/// `debug_trace` feature enabled, and to a
/// [`NoopTaskTrace`](crate::task_trace::NoopTaskTrace) otherwise.
#[macro_export]
macro_rules! task_trace {
    () => {{
        #[cfg(feature = "debug_trace")]
        {
            let s = $crate::async_backtrace::location!();
            $crate::task_trace::TaskTrace::new(s)
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            $crate::task_trace::NoopTaskTrace::new()
        }
    }};
}

/// Creates an anonymous task trace guard that lives until the end of the
/// current scope.
#[macro_export]
macro_rules! scoped_task_trace {
    () => {
        let _task_trace = $crate::task_trace!();
    };
}

/// Returns a dump of the traces of all registered task contexts.
///
/// Expands to an empty string when the `debug_trace` feature is disabled.
#[macro_export]
macro_rules! dump_task_trace {
    () => {{
        #[cfg(feature = "debug_trace")]
        {
            $crate::task_trace::TaskTrace::dump_task_trace()
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            String::new()
        }
    }};
}

/// Returns the recorded backtrace of the current task.
///
/// Expands to an empty string when the `debug_trace` feature is disabled.
#[macro_export]
macro_rules! task_backtrace {
    () => {{
        #[cfg(feature = "debug_trace")]
        {
            $crate::task_trace::TaskTrace::backtrace()
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            String::new()
        }
    }};
}

/// Returns the ID of the current async task.
#[macro_export]
macro_rules! this_task_id {
    () => {{ $crate::task_trace::this_task_id() }};
}
