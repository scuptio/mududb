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

unsafe impl<T> Send for AMutex<T> where T: ?Sized + Send {}
unsafe impl<T> Sync for AMutex<T> where T: ?Sized + Send {}
unsafe impl<T> Sync for AMutexGuard<'_, T> where T: ?Sized + Send + Sync {}
unsafe impl<'a, T> Sync for MappedAMutexGuard<'a, T> where T: ?Sized + Sync + 'a {}
unsafe impl<'a, T> Send for MappedAMutexGuard<'a, T> where T: ?Sized + Send + 'a {}

#[derive(Debug)]
pub struct TryLockError(pub ());

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
        AMutexGuard {
            inner: self.inner.lock().await,
        }
    }

    pub fn try_lock(&self) -> Option<AMutexGuard<'_, T>> {
        self.inner
            .try_lock()
            .ok()
            .map(|inner| AMutexGuard { inner })
    }

    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        self.inner.into_inner()
    }
}

impl<T> From<T> for AMutex<T> {
    fn from(value: T) -> Self {
        Self::new(value)
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

impl<T: ?Sized + fmt::Debug> fmt::Debug for AMutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<'a, T: ?Sized> AMutexGuard<'a, T> {
    pub fn map<U, F>(this: Self, f: F) -> MappedAMutexGuard<'a, U>
    where
        U: ?Sized,
        F: FnOnce(&mut T) -> &mut U,
    {
        MappedAMutexGuard {
            inner: TokioMutexGuard::map(this.inner, f),
        }
    }

    pub fn try_map<U, F>(this: Self, f: F) -> Result<MappedAMutexGuard<'a, U>, Self>
    where
        U: ?Sized,
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        match TokioMutexGuard::try_map(this.inner, f) {
            Ok(inner) => Ok(MappedAMutexGuard { inner }),
            Err(inner) => Err(AMutexGuard { inner }),
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
    pub fn map<U, F>(this: Self, f: F) -> MappedAMutexGuard<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        MappedAMutexGuard {
            inner: TokioMappedMutexGuard::map(this.inner, f),
        }
    }

    pub fn try_map<U, F>(this: Self, f: F) -> Result<MappedAMutexGuard<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        match TokioMappedMutexGuard::try_map(this.inner, f) {
            Ok(inner) => Ok(MappedAMutexGuard { inner }),
            Err(inner) => Err(MappedAMutexGuard { inner }),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn amutex_lock_and_modify() {
        let mutex = AMutex::new(0);
        {
            let mut guard = mutex.lock().await;
            *guard += 1;
        }
        let guard = mutex.lock().await;
        assert_eq!(*guard, 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn amutex_try_lock_returns_none_when_held() {
        let mutex = AMutex::new(0);
        let guard = mutex.lock().await;
        assert!(mutex.try_lock().is_none());
        drop(guard);
        assert!(mutex.try_lock().is_some());
    }

    #[test]
    fn amutex_into_inner_yields_value() {
        let mutex = AMutex::new(42);
        assert_eq!(mutex.into_inner(), 42);
    }

    #[test]
    fn amutex_from_value() {
        let mutex: AMutex<i32> = AMutex::from(7);
        assert_eq!(mutex.into_inner(), 7);
    }

    #[test]
    fn amutex_default() {
        let mutex: AMutex<i32> = AMutex::default();
        assert_eq!(mutex.into_inner(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn amutex_guard_map() {
        let mutex = AMutex::new((1, 2));
        let guard = mutex.lock().await;
        let mut mapped = AMutexGuard::map(guard, |tuple| &mut tuple.1);
        assert_eq!(*mapped, 2);
        *mapped = 7;
        drop(mapped);
        let guard = mutex.lock().await;
        assert_eq!(guard.1, 7);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn amutex_guard_try_map_some() {
        let mutex = AMutex::new((1, 2));
        let guard = mutex.lock().await;
        let mut mapped = AMutexGuard::try_map(guard, |tuple| Some(&mut tuple.1)).unwrap();
        assert_eq!(*mapped, 2);
        *mapped = 9;
        drop(mapped);
        let guard = mutex.lock().await;
        assert_eq!(guard.1, 9);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn amutex_guard_try_map_none_returns_guard() {
        let mutex = AMutex::new((1, 2));
        let guard = mutex.lock().await;
        let mut guard =
            AMutexGuard::try_map(guard, |_tuple: &mut (i32, i32)| -> Option<&mut i32> {
                None
            })
            .unwrap_err();
        assert_eq!(*guard, (1, 2));
        *guard = (3, 4);
        drop(guard);
        let guard = mutex.lock().await;
        assert_eq!(*guard, (3, 4));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn amutex_debug_and_display() {
        let mutex = AMutex::new(123);
        let guard = mutex.lock().await;
        let _ = format!("{:?}", mutex);
        let _ = format!("{}", guard);
        let _ = format!("{:?}", guard);
    }
}
