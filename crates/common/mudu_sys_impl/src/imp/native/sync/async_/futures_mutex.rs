use futures::lock::{Mutex as FuturesMutex, MutexGuard as FuturesMutexGuard};
use std::fmt;
use std::ops::{Deref, DerefMut};

/// Async mutex backed by `futures::lock::Mutex`.
///
/// We keep this wrapper in `mudu_sys` so higher-level crates do not depend on
/// `futures` directly. This type is specifically useful on execution paths that
/// do not run on a plain Tokio scheduler. In our io_uring/custom-runtime path
/// we observed `tokio::sync::Mutex::lock().await` stall even without real lock
/// contention, while the futures mutex remained stable.
pub struct FMutex<T: ?Sized> {
    inner: FuturesMutex<T>,
}

pub struct FMutexGuard<'a, T: ?Sized> {
    inner: FuturesMutexGuard<'a, T>,
}

unsafe impl<T> Send for FMutex<T> where T: ?Sized + Send {}
unsafe impl<T> Sync for FMutex<T> where T: ?Sized + Send {}

impl<T: ?Sized> FMutex<T> {
    pub fn new(t: T) -> Self
    where
        T: Sized,
    {
        Self {
            inner: FuturesMutex::new(t),
        }
    }

    pub async fn lock(&self) -> FMutexGuard<'_, T> {
        FMutexGuard {
            inner: self.inner.lock().await,
        }
    }

    pub fn try_lock(&self) -> Option<FMutexGuard<'_, T>> {
        self.inner.try_lock().map(|inner| FMutexGuard { inner })
    }

    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        self.inner.into_inner()
    }
}

impl<T> From<T> for FMutex<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T> Default for FMutex<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for FMutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized> Deref for FMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<T: ?Sized> DerefMut for FMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for FMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for FMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn fmutex_lock_and_modify() {
        let mutex = FMutex::new(0);
        block_on(async {
            {
                let mut guard = mutex.lock().await;
                *guard += 1;
            }
            let guard = mutex.lock().await;
            assert_eq!(*guard, 1);
        });
    }

    #[test]
    fn fmutex_try_lock() {
        let mutex = FMutex::new(0);
        block_on(async {
            let guard = mutex.lock().await;
            assert!(mutex.try_lock().is_none());
            drop(guard);
            assert!(mutex.try_lock().is_some());
        });
    }

    #[test]
    fn fmutex_into_inner() {
        let mutex = FMutex::new("hello");
        assert_eq!(mutex.into_inner(), "hello");
    }

    #[test]
    fn fmutex_from_value() {
        let mutex: FMutex<&str> = FMutex::from("x");
        assert_eq!(mutex.into_inner(), "x");
    }

    #[test]
    fn fmutex_default() {
        let mutex: FMutex<i32> = FMutex::default();
        assert_eq!(mutex.into_inner(), 0);
    }

    #[test]
    fn fmutex_guard_deref_and_display() {
        let mutex = FMutex::new(42);
        block_on(async {
            let guard = mutex.lock().await;
            assert_eq!(*guard, 42);
            let displayed = format!("{}", guard);
            assert_eq!(displayed, "42");
        });
    }
}
