pub mod async_;
pub mod blocking;
pub mod std_mutex;
pub mod std_rwlock;
pub mod unique_inner;

// Deprecated aliases for backward compatibility
#[deprecated(note = "use mudu_sys::sync::async_::mutex instead")]
pub mod tokio_mutex {
    pub use crate::sync::async_::mutex::*;
}
#[deprecated(note = "use mudu_sys::sync::async_::rwlock instead")]
pub mod tokio_rwlock {
    pub use crate::sync::async_::rwlock::*;
}
#[deprecated(note = "use mudu_sys::sync::async_::notify instead")]
pub mod tokio_notify {
    pub use crate::sync::async_::notify::*;
}
#[deprecated(note = "use mudu_sys::sync::async_::notify_wait instead")]
pub mod notify_wait {
    pub use crate::sync::async_::notify_wait::*;
}
#[deprecated(note = "use mudu_sys::sync::async_::stop_flag instead")]
pub mod stop_flag {
    pub use crate::sync::async_::stop_flag::*;
}
#[deprecated(note = "use mudu_sys::sync::async_::futures_mutex instead")]
pub mod futures_mutex {
    pub use crate::sync::async_::futures_mutex::*;
}
#[deprecated(note = "use mudu_sys::sync::async_::async_task instead")]
pub mod async_task {
    pub use crate::sync::async_::async_task::*;
}

pub use crate::sync::std_mutex::{SCondvar, SMutex, SMutexGuard};
pub use crate::sync::std_rwlock::{SRwLock, SRwLockReadGuard, SRwLockWriteGuard};

#[cfg(not(target_arch = "wasm32"))]
pub use crate::sync::async_::*;
pub use crate::sync::blocking::*;
