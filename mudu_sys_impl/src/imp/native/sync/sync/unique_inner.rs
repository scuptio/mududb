use crate::imp::sync::std_mutex::SMutex;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
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

    pub fn inner_into(&self) -> RS<T> {
        let mut guard = self.inner.lock()?;
        let mut ret = None;
        std::mem::swap(&mut ret, &mut guard);
        ret.ok_or_else(|| {
            mudu_error!(
                ErrorCode::Internal,
                "UniqueInner::inner_into called more than once"
            )
        })
    }

    pub fn map_inner<R, M: Fn(&T) -> R>(&self, map: M) -> Option<R> {
        self.inner.lock().ok()?.as_ref().map(map)
    }
}

unsafe impl<T> Sync for UniqueInner<T> {}
unsafe impl<T> Send for UniqueInner<T> {}
