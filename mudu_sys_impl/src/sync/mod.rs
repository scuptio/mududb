//! Public synchronization primitives and blocking IO helpers.
#![allow(missing_docs)]
pub use crate::imp::sync::async_;
pub use crate::imp::sync::blocking;
pub use crate::imp::sync::std_mutex;
pub use crate::imp::sync::std_rwlock;
pub use crate::imp::sync::sync as sync_;

pub use crate::imp::sync::std_mutex::{SCondvar, SMutex, SMutexGuard};
pub use crate::imp::sync::std_rwlock::{SRwLock, SRwLockReadGuard, SRwLockWriteGuard};
