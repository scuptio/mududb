//! Minimal system services available to WebAssembly applications.

/// Random value and UUID generation.
pub mod random {
    pub use uuid::Uuid;

    /// Generates a random version 4 UUID.
    pub fn uuid_v4() -> Uuid {
        Uuid::new_v4()
    }

    /// Generates a random version 4 UUID as a string.
    pub fn next_uuid_v4_string() -> String {
        uuid_v4().to_string()
    }
}

/// Portable synchronization primitives.
pub mod sync {
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use std::ops::{Deref, DerefMut};
    use std::sync::{Mutex, MutexGuard};

    /// Mutex with the error behavior expected by MuduDB APIs.
    #[derive(Debug, Default)]
    pub struct SMutex<T: ?Sized> {
        inner: Mutex<T>,
    }

    impl<T> SMutex<T> {
        /// Creates a mutex containing `value`.
        pub const fn new(value: T) -> Self {
            Self {
                inner: Mutex::new(value),
            }
        }
    }

    impl<T: ?Sized> SMutex<T> {
        /// Locks the mutex.
        pub fn lock(&self) -> RS<SMutexGuard<'_, T>> {
            self.inner
                .lock()
                .map(|inner| SMutexGuard { inner })
                .map_err(|_| mudu_error!(ErrorCode::Mutex, "mutex poisoned"))
        }

        /// Attempts to lock the mutex without blocking.
        pub fn try_lock(&self) -> Option<SMutexGuard<'_, T>> {
            self.inner
                .try_lock()
                .ok()
                .map(|inner| SMutexGuard { inner })
        }
    }

    /// Guard returned by [`SMutex::lock`].
    pub struct SMutexGuard<'a, T: ?Sized> {
        inner: MutexGuard<'a, T>,
    }

    impl<T: ?Sized> Deref for SMutexGuard<'_, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }

    impl<T: ?Sized> DerefMut for SMutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.inner
        }
    }
}

/// Time types and clocks supplied by WASI.
pub mod time {
    pub use chrono::{DateTime, Utc};
    pub use std::time::{Instant, SystemTime};

    /// Returns the current monotonic instant.
    pub fn instant_now() -> Instant {
        Instant::now()
    }

    /// Returns the current system time.
    pub fn system_time_now() -> SystemTime {
        SystemTime::now()
    }

    /// Returns the current UTC time.
    pub fn utc_now() -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
mod random_test;

#[cfg(test)]
mod sync_test;

#[cfg(test)]
mod time_test;
