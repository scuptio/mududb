use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{
    RwLock as StdRwLock, RwLockReadGuard as StdRwLockReadGuard,
    RwLockWriteGuard as StdRwLockWriteGuard,
};

pub struct SRwLock<T: ?Sized> {
    inner: StdRwLock<T>,
}

pub struct SRwLockReadGuard<'a, T: ?Sized + 'a> {
    inner: StdRwLockReadGuard<'a, T>,
}

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
    pub const fn new(t: T) -> SRwLock<T> {
        Self {
            inner: StdRwLock::new(t),
        }
    }
}

impl<T: ?Sized> SRwLock<T> {
    pub fn read(&self) -> RS<SRwLockReadGuard<'_, T>> {
        let r = self.inner.read();
        match r {
            Ok(r) => Ok(SRwLockReadGuard { inner: r }),
            Err(_e) => Err(mudu_error!(ErrorCode::Mutex, "rwlock read error")),
        }
    }

    pub fn write(&self) -> RS<SRwLockWriteGuard<'_, T>> {
        let r = self.inner.write();
        match r {
            Ok(w) => Ok(SRwLockWriteGuard { inner: w }),
            Err(_e) => Err(mudu_error!(ErrorCode::Mutex, "rwlock write error")),
        }
    }

    pub fn try_read(&self) -> Option<SRwLockReadGuard<'_, T>> {
        let r = self.inner.try_read();
        match r {
            Ok(g) => Some(SRwLockReadGuard { inner: g }),
            Err(_e) => None,
        }
    }

    pub fn try_write(&self) -> Option<SRwLockWriteGuard<'_, T>> {
        let r = self.inner.try_write();
        match r {
            Ok(g) => Some(SRwLockWriteGuard { inner: g }),
            Err(_e) => None,
        }
    }
}

impl<T> SRwLock<T> {
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
}
