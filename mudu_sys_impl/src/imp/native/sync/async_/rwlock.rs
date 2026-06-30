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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[tokio::test(flavor = "current_thread")]
    async fn arwlock_read_write_read() {
        let lock = ARwLock::new(0);
        {
            let guard = lock.read().await;
            assert_eq!(*guard, 0);
        }
        {
            let mut guard = lock.write().await;
            *guard = 5;
        }
        {
            let guard = lock.read().await;
            assert_eq!(*guard, 5);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn arwlock_multiple_read_guards_coexist() {
        let lock = ARwLock::new(7);
        let g1 = lock.read().await;
        let g2 = lock.read().await;
        assert_eq!(*g1, 7);
        assert_eq!(*g2, 7);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn arwlock_write_excludes_read() {
        let lock = Arc::new(ARwLock::new(0));
        let write_guard = lock.write().await;

        let (tx, mut rx) = mpsc::channel(2);
        let lock2 = lock.clone();
        let handle = tokio::spawn(async move {
            tx.send("before-read").await.unwrap();
            let _read_guard = lock2.read().await;
            tx.send("after-read").await.unwrap();
        });

        assert_eq!(rx.recv().await, Some("before-read"));
        assert!(tokio::time::timeout(Duration::from_millis(50), rx.recv())
            .await
            .is_err());

        drop(write_guard);
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(500), rx.recv())
                .await
                .unwrap(),
            Some("after-read")
        );
        handle.await.unwrap();
    }

    #[test]
    fn arwlock_into_inner_yields_value() {
        let lock = ARwLock::new(42);
        assert_eq!(lock.into_inner(), 42);
    }

    #[test]
    fn arwlock_from_value() {
        let lock: ARwLock<i32> = ARwLock::from(7);
        assert_eq!(lock.into_inner(), 7);
    }

    #[test]
    fn arwlock_default() {
        let lock: ARwLock<i32> = ARwLock::default();
        assert_eq!(lock.into_inner(), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn arwlock_debug_and_display() {
        let lock = ARwLock::new(123);
        let guard = lock.read().await;
        let _ = format!("{:?}", lock);
        let _ = format!("{}", guard);
        let _ = format!("{:?}", guard);
    }
}
