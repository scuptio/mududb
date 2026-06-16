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
            $crate::task::trace::TaskTrace::dump_task_trace()
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
            $crate::task::trace::TaskTrace::backtrace()
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            String::new()
        }
    }};
}

#[macro_export]
macro_rules! this_task_id {
    () => {{ $crate::task::trace::this_task_id() }};
}
