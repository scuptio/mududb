use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{Condvar as StdCondvar, Mutex as StdSyncMutex, Mutex, MutexGuard as StdMutexGuard};

pub struct SMutex<T: ?Sized> {
    inner: StdSyncMutex<T>,
}

unsafe impl<T: ?Sized> Send for SMutex<T> {}

unsafe impl<T: ?Sized> Sync for SMutex<T> {}

pub struct SMutexGuard<'a, T: ?Sized + 'a> {
    inner: StdMutexGuard<'a, T>,
}

pub struct SCondvar {
    inner: StdCondvar,
}

//impl<T: ?Sized> !Send for SMutexGuard<'_, T> {}

unsafe impl<T: ?Sized + Sync> Sync for SMutexGuard<'_, T> {}

impl<T> SMutex<T> {
    pub const fn new(t: T) -> SMutex<T> {
        Self {
            inner: StdSyncMutex::new(t),
        }
    }
}

impl<T: ?Sized> SMutex<T> {
    pub fn lock(&self) -> RS<SMutexGuard<'_, T>> {
        let r = self.inner.lock();
        match r {
            Ok(r) => Ok(SMutexGuard { inner: r }),
            Err(_e) => Err(mudu_error!(ErrorCode::Mutex, "")),
        }
    }

    pub fn try_lock(&self) -> Option<SMutexGuard<'_, T>> {
        let r = self.inner.try_lock();
        match r {
            Ok(g) => Some(SMutexGuard { inner: g }),
            Err(_e) => None,
        }
    }
}

impl Default for SCondvar {
    fn default() -> Self {
        Self::new()
    }
}

impl SCondvar {
    pub const fn new() -> Self {
        Self {
            inner: StdCondvar::new(),
        }
    }

    pub fn wait<'a, T>(&self, guard: SMutexGuard<'a, T>) -> RS<SMutexGuard<'a, T>> {
        self.inner
            .wait(guard.inner)
            .map(|inner| SMutexGuard { inner })
            .map_err(|_| mudu_error!(ErrorCode::Mutex, ""))
    }

    pub fn notify_all(&self) {
        self.inner.notify_all();
    }
}

impl<T> SMutex<T> {
    pub fn into_inner(self) -> RS<T> {
        let r = self.inner.into_inner();
        match r {
            Ok(t) => Ok(t),
            Err(_e) => Err(mudu_error!(ErrorCode::Mutex, "")),
        }
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for SMutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized> Deref for SMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.inner.deref()
    }
}

impl<T: ?Sized> DerefMut for SMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner.deref_mut()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for SMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for SMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: Default> Default for SMutex<T> {
    fn default() -> SMutex<T> {
        Self {
            inner: Mutex::new(Default::default()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::SMutex;

    #[test]
    fn mutex_lock_unlock() {
        let mutex = SMutex::new(0);
        {
            let mut guard = mutex.lock().unwrap();
            *guard += 1;
        }
        let guard = mutex.lock().unwrap();
        assert_eq!(*guard, 1);
    }

    #[test]
    fn mutex_try_lock() {
        let mutex = SMutex::new(0);
        let guard = mutex.try_lock().unwrap();
        assert_eq!(*guard, 0);
        assert!(mutex.try_lock().is_none());
    }
}
