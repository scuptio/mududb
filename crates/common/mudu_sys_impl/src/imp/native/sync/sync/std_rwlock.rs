use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{
    RwLock as StdRwLock, RwLockReadGuard as StdRwLockReadGuard,
    RwLockWriteGuard as StdRwLockWriteGuard,
};

/// Blocking read-write lock over [`std::sync::RwLock`]; lock poisoning is
/// reported as an error instead of a panic.
pub struct SRwLock<T: ?Sized> {
    inner: StdRwLock<T>,
}

/// RAII guard holding a shared read lock; releases on drop.
pub struct SRwLockReadGuard<'a, T: ?Sized + 'a> {
    inner: StdRwLockReadGuard<'a, T>,
}

/// RAII guard holding the exclusive write lock; releases on drop.
pub struct SRwLockWriteGuard<'a, T: ?Sized + 'a> {
    inner: StdRwLockWriteGuard<'a, T>,
}

unsafe impl<T: ?Sized + Send> Send for SRwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for SRwLock<T> {}

unsafe impl<T: ?Sized + Sync> Sync for SRwLockReadGuard<'_, T> {}
unsafe impl<T: ?Sized + Send> Send for SRwLockReadGuard<'_, T> {}

unsafe impl<T: ?Sized + Sync> Sync for SRwLockWriteGuard<'_, T> {}
unsafe impl<T: ?Sized + Send> Send for SRwLockWriteGuard<'_, T> {}

impl<T> SRwLock<T> {
    /// Creates a new read-write lock protecting `t`.
    pub const fn new(t: T) -> SRwLock<T> {
        Self {
            inner: StdRwLock::new(t),
        }
    }
}

impl<T: ?Sized> SRwLock<T> {
    /// Acquires a shared read lock, blocking until no writer holds or
    /// awaits the lock; fails if the lock is poisoned.
    pub fn read(&self) -> RS<SRwLockReadGuard<'_, T>> {
        let r = self.inner.read();
        match r {
            Ok(r) => Ok(SRwLockReadGuard { inner: r }),
            Err(_e) => Err(mudu_error!(ErrorCode::Mutex, "rwlock read error")),
        }
    }

    /// Acquires the exclusive write lock, blocking until no other reader or
    /// writer holds it; fails if the lock is poisoned.
    pub fn write(&self) -> RS<SRwLockWriteGuard<'_, T>> {
        let r = self.inner.write();
        match r {
            Ok(w) => Ok(SRwLockWriteGuard { inner: w }),
            Err(_e) => Err(mudu_error!(ErrorCode::Mutex, "rwlock write error")),
        }
    }

    /// Attempts to acquire a read lock without blocking; returns `None` if
    /// it is locked or poisoned.
    pub fn try_read(&self) -> Option<SRwLockReadGuard<'_, T>> {
        let r = self.inner.try_read();
        match r {
            Ok(g) => Some(SRwLockReadGuard { inner: g }),
            Err(_e) => None,
        }
    }

    /// Attempts to acquire the write lock without blocking; returns `None`
    /// if it is locked or poisoned.
    pub fn try_write(&self) -> Option<SRwLockWriteGuard<'_, T>> {
        let r = self.inner.try_write();
        match r {
            Ok(g) => Some(SRwLockWriteGuard { inner: g }),
            Err(_e) => None,
        }
    }
}

impl<T> SRwLock<T> {
    /// Consumes the lock and returns the protected value; fails if the lock
    /// is poisoned.
    pub fn into_inner(self) -> RS<T> {
        let r = self.inner.into_inner();
        match r {
            Ok(t) => Ok(t),
            Err(_e) => Err(mudu_error!(ErrorCode::Mutex, "rwlock into_inner error")),
        }
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for SRwLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized> Deref for SRwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.inner.deref()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for SRwLockReadGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for SRwLockReadGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized> Deref for SRwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.inner.deref()
    }
}

impl<T: ?Sized> DerefMut for SRwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner.deref_mut()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for SRwLockWriteGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for SRwLockWriteGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: Default> Default for SRwLock<T> {
    fn default() -> SRwLock<T> {
        Self {
            inner: StdRwLock::new(Default::default()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::SRwLock;

    #[test]
    fn rwlock_read_write() {
        let lock = SRwLock::new(0);
        {
            let read = lock.read().unwrap();
            assert_eq!(*read, 0);
        }
        {
            let mut write = lock.write().unwrap();
            *write += 1;
        }
        {
            let read = lock.read().unwrap();
            assert_eq!(*read, 1);
        }
    }

    #[test]
    fn rwlock_try_write() {
        let lock = SRwLock::new(0);
        let write = lock.try_write().unwrap();
        assert_eq!(*write, 0);
        assert!(lock.try_write().is_none());
        assert!(lock.try_read().is_none());
    }

    #[test]
    fn rwlock_try_locks_formatting_default_and_into_inner() {
        let lock = SRwLock::new(1);
        let read = lock.read().unwrap();
        assert_eq!(*read, 1);
        assert!(lock.try_write().is_none());
        assert!(lock.try_read().is_some());
        let _ = format!("{:?}", read);
        let _ = format!("{}", read);
        drop(read);

        let mut write = lock.write().unwrap();
        *write = 2;
        let _ = format!("{:?}", write);
        let _ = format!("{}", write);
        assert!(lock.try_read().is_none());
        drop(write);

        let default_lock = SRwLock::<u32>::default();
        let _ = format!("{:?}", default_lock);
        assert_eq!(default_lock.into_inner().unwrap(), 0);
        assert_eq!(lock.into_inner().unwrap(), 2);
    }

    #[test]
    fn poisoned_rwlock_fails_read_write_and_into_inner() {
        let lock = SRwLock::new(0);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _write = lock.write().unwrap();
            panic!("poison the rwlock");
        }));
        assert!(lock.read().is_err());
        assert!(lock.write().is_err());
        assert!(lock.into_inner().is_err());
    }
}
