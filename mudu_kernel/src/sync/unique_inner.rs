use crate::sync::s_mutex::SMutex;
use std::sync::Arc;

#[derive(Clone)]
pub struct UniqueInner<T> {
    inner: Arc<SMutex<Option<T>>>,
}

impl<T> UniqueInner<T> {
    pub fn new(t: T) -> Self {
        Self {
            inner: Arc::new(SMutex::new(Some(t))),
        }
    }

    pub fn inner_into(&self) -> T {
        let r = self.inner.lock();
        let mut guard = match r {
            Ok(g) => g,
            Err(e) => {
                panic!("lock error {}", e)
            }
        };
        let mut ret = None;
        std::mem::swap(&mut ret, &mut guard);
        match ret {
            Some(t) => t,
            None => {
                panic!("inner into can be invoked only once")
            }
        }
    }

    pub fn map_inner<R, M: Fn(&T) -> R>(&self, map: M) -> Option<R> {
        let r = self.inner.lock();
        let guard = match r {
            Ok(g) => g,
            Err(e) => {
                panic!("lock error {}", e)
            }
        };
        (*guard).as_ref().map(map)
    }
}

unsafe impl<T> Sync for UniqueInner<T> {}
unsafe impl<T> Send for UniqueInner<T> {}
