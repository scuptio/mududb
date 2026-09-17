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

#[macro_export]
macro_rules! scoped_task_trace {
    () => {
        let _task_trace = $crate::task_trace!();
    };
}

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

#[macro_export]
macro_rules! this_task_id {
    () => {{ $crate::task_trace::this_task_id() }};
}
