use std::fmt;
use std::ops::{Deref, DerefMut};

use tokio::sync::{
    MappedMutexGuard as TokioMappedMutexGuard, Mutex as TokioMutex, MutexGuard as TokioMutexGuard,
};

pub struct AMutex<T: ?Sized> {
    inner: TokioMutex<T>,
}

pub struct AMutexGuard<'a, T: ?Sized> {
    inner: TokioMutexGuard<'a, T>,
}

pub struct MappedAMutexGuard<'a, T: ?Sized> {
    inner: TokioMappedMutexGuard<'a, T>,
}

// As long as T: Send, it's fine to send and share Mutex<T> between threads.
// If T was not Send, sending and sharing a Mutex<T> would be bad, since you can
// access T through Mutex<T>.
unsafe impl<T> Send for AMutex<T>
where
    T: ?Sized + Send,
{}
unsafe impl<T> Sync for AMutex<T> where T: ?Sized + Send {}
unsafe impl<T> Sync for AMutexGuard<'_, T> where T: ?Sized + Send + Sync {}

unsafe impl<'a, T> Sync for MappedAMutexGuard<'a, T> where T: ?Sized + Sync + 'a {}
unsafe impl<'a, T> Send for MappedAMutexGuard<'a, T> where T: ?Sized + Send + 'a {}

#[derive(Debug)]
pub struct TryLockError(pub(super) ());

impl fmt::Display for TryLockError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "operation would block")
    }
}

impl<T: ?Sized> AMutex<T> {
    pub fn new(t: T) -> Self
    where
        T: Sized,
    {
        Self {
            inner: TokioMutex::new(t),
        }
    }

    pub const fn const_new(t: T) -> Self
    where
        T: Sized,
    {
        Self {
            inner: TokioMutex::const_new(t),
        }
    }

    pub async fn lock(&self) -> AMutexGuard<'_, T> {
        let inner = self.inner.lock().await;
        AMutexGuard { inner }
    }

    pub fn try_lock(&self) -> Option<AMutexGuard<'_, T>> {
        let r = self.inner.try_lock();
        match r {
            Ok(g) => Some(AMutexGuard { inner: g }),
            Err(_e) => None,
        }
    }

    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        self.inner.into_inner()
    }
}

impl<T> From<T> for AMutex<T> {
    fn from(s: T) -> Self {
        Self::new(s)
    }
}

impl<T> Default for AMutex<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: ?Sized> std::fmt::Debug for AMutex<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

impl<'a, T: ?Sized> AMutexGuard<'a, T> {
    #[inline]
    pub fn map<U, F>(this: Self, f: F) -> MappedAMutexGuard<'a, U>
    where
        U: ?Sized,
        F: FnOnce(&mut T) -> &mut U,
    {
        let inner = TokioMutexGuard::map(this.inner, f);
        MappedAMutexGuard { inner }
    }

    #[inline]
    pub fn try_map<U, F>(this: Self, f: F) -> Result<MappedAMutexGuard<'a, U>, Self>
    where
        U: ?Sized,
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        let r = TokioMutexGuard::try_map(this.inner, f);
        match r {
            Ok(r) => Ok(MappedAMutexGuard { inner: r }),
            Err(e) => Err(AMutexGuard { inner: e }),
        }
    }
}

impl<T: ?Sized> Deref for AMutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<T: ?Sized> DerefMut for AMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for AMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for AMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<'a, T: ?Sized> MappedAMutexGuard<'a, T> {
    #[inline]
    pub fn map<U, F>(this: Self, f: F) -> MappedAMutexGuard<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        let inner = TokioMappedMutexGuard::map(this.inner, f);
        MappedAMutexGuard { inner }
    }

    #[inline]
    pub fn try_map<U, F>(this: Self, f: F) -> Result<MappedAMutexGuard<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        let r = TokioMappedMutexGuard::try_map(this.inner, f);
        match r {
            Ok(r) => Ok(MappedAMutexGuard { inner: r }),
            Err(e) => Err(MappedAMutexGuard { inner: e }),
        }
    }
}

impl<T: ?Sized> Deref for MappedAMutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<T: ?Sized> DerefMut for MappedAMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for MappedAMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for MappedAMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}
