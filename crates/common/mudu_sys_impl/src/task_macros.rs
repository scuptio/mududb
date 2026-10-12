/// Creates a [`TaskTrace`](crate::task::trace::TaskTrace) at the current
/// source location when the `debug_trace` feature is enabled, or a no-op
/// tracer otherwise.
#[macro_export]
macro_rules! task_trace {
    () => {{
        #[cfg(feature = "debug_trace")]
        {
            let s = async_backtrace::location!();
            $crate::task::trace::TaskTrace::new(s)
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            $crate::task::trace::NoopTaskTrace::new()
        }
    }};
}

/// Binds a scoped tracer (see [`task_trace!`](crate::task_trace)) to an
/// anonymous local that drops at the end of the enclosing scope.
#[macro_export]
macro_rules! scoped_task_trace {
    () => {
        let _task_trace = $crate::task_trace!();
    };
}

/// Renders the contexts of all registered tasks as text when the
/// `debug_trace` feature is enabled; returns an empty string otherwise.
#[macro_export]
macro_rules! dump_task_trace {
    () => {{
        #[cfg(feature = "debug_trace")]
        {
            $crate::task::trace::TaskTrace::dump_task_trace()
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            String::new()
        }
    }};
}

/// Renders the current task's backtrace when the `debug_trace` feature is
/// enabled; returns an empty string otherwise.
#[macro_export]
macro_rules! task_backtrace {
    () => {{
        #[cfg(feature = "debug_trace")]
        {
            $crate::task::trace::TaskTrace::backtrace()
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            String::new()
        }
    }};
}

/// Returns the id of the current task; panics if there is none.
#[macro_export]
macro_rules! this_task_id {
    () => {{
        $crate::task::trace::this_task_id()
    }};
}
