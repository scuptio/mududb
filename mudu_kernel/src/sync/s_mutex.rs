use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{Mutex as StdSyncMutex, Mutex, MutexGuard as StdMutexGuard};

pub struct SMutex<T: ?Sized> {
    inner: StdSyncMutex<T>,
}

unsafe impl<T: ?Sized> Send for SMutex<T> {}

unsafe impl<T: ?Sized> Sync for SMutex<T> {}

pub struct SMutexGuard<'a, T: ?Sized + 'a> {
    inner: StdMutexGuard<'a, T>,
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
            Err(_e) => Err(m_error!(EC::MutexError, "")),
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

impl<T: ?Sized + Default> Default for SMutex<T> {
    fn default() -> SMutex<T> {
        Self {
            inner: Mutex::new(Default::default()),
        }
    }
}
