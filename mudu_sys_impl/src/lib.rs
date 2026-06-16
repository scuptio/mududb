#![feature(sync_unsafe_cell)]

pub mod common;
pub mod contract;
pub use mudu_sys_contract::perf;
pub mod env;
pub mod fs;
pub mod imp;
pub mod io;
pub mod net;
pub mod process;
pub mod provider;
pub mod sync;
pub mod sys_context;
pub mod task;
mod task_macros;

pub mod sys {
    pub use crate::imp;
}

// Expose native/sim at the crate root as requested by the refactoring plan.
pub use crate::imp::{native, sim};

pub use crate::imp::env::Sys;
pub use crate::sys_context::{SysContext, default_sys_context};

#[cfg(not(target_arch = "wasm32"))]
pub use crate::sync::async_::{Notifier, NotifyWait, Waiter, notify_wait};
pub use crate::sync::blocking::{
    ChannelReceiver, ChannelSender, ChannelSyncSender, channel, close_fd, eventfd, notify_eventfd,
    read_eventfd, sync_channel,
};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::task::async_::{
    CurrentThreadTaskRuntime, LocalTaskSet, PollTaskIdGuard, TaskFailed, block_on_async_current,
    block_on_tokio_current_thread, build_current_thread_runtime, build_multi_thread_runtime,
    current_poll_task_id, has_tokio_runtime, sleep, spawn_blocking, spawn_local_detached,
    spawn_local_task, spawn_task, spawn_task_detached, this_task_id, timeout, try_this_task_id,
    wait_for_shutdown_signal,
};
pub use crate::task::context::TaskContext;
pub use crate::task::id::{TaskID, new_task_id};
pub use crate::task::sync::{SJoinHandle, sleep_blocking, spawn_thread, spawn_thread_named};
pub use crate::task::trace::{NoopTaskTrace, TaskTrace};
#[cfg(target_os = "linux")]
pub mod uring {
    pub use crate::io::iouring::*;
}

#[cfg(not(target_arch = "wasm32"))]
pub use tokio;

pub mod server;

pub mod random {
    pub use crate::contract::random::{next_uuid_v4_string, uuid_v4};
}

pub mod time {
    pub use crate::contract::time::{instant_now, system_time_now, utc_now};
}

pub mod env_var {
    pub use crate::contract::env_var::*;
}

#[deprecated(note = "use mudu_sys::net::sync instead")]
pub mod sync_net {
    pub use crate::net::sync::*;
}

#[deprecated(note = "use mudu_sys::fs::sync instead")]
pub mod fs_sync {
    pub use crate::fs::sync::*;
}

pub fn io_uring_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        crate::io::iouring::IoUring::new(8).is_ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}
