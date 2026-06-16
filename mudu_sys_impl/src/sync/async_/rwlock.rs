use std::fmt;
use std::ops::{Deref, DerefMut};

use tokio::sync::{
    RwLock as TokioRwLock, RwLockReadGuard as TokioRwLockReadGuard,
    RwLockWriteGuard as TokioRwLockWriteGuard,
};

pub struct ARwLock<T: ?Sized> {
    inner: TokioRwLock<T>,
}

pub struct ARwLockReadGuard<'a, T: ?Sized> {
    inner: TokioRwLockReadGuard<'a, T>,
}

pub struct ARwLockWriteGuard<'a, T: ?Sized> {
    inner: TokioRwLockWriteGuard<'a, T>,
}

unsafe impl<T> Send for ARwLock<T> where T: ?Sized + Send + Sync {}
unsafe impl<T> Sync for ARwLock<T> where T: ?Sized + Send + Sync {}

impl<T: ?Sized> ARwLock<T> {
    pub fn new(t: T) -> Self
    where
        T: Sized,
    {
        Self {
            inner: TokioRwLock::new(t),
        }
    }

    pub async fn read(&self) -> ARwLockReadGuard<'_, T> {
        ARwLockReadGuard {
            inner: self.inner.read().await,
        }
    }

    pub async fn write(&self) -> ARwLockWriteGuard<'_, T> {
        ARwLockWriteGuard {
            inner: self.inner.write().await,
        }
    }

    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        self.inner.into_inner()
    }
}

impl<T> From<T> for ARwLock<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T> Default for ARwLock<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for ARwLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized> Deref for ARwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for ARwLockReadGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for ARwLockReadGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized> Deref for ARwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<T: ?Sized> DerefMut for ARwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for ARwLockWriteGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for ARwLockWriteGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}
