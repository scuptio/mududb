use super::*;

impl Clone for VecCursor {
    // `SMutex::lock` returns `RS`; `Clone` cannot propagate the error, and the
    // cursor is not expected to be poisoned in normal operation, so unwrap here.
    #[allow(clippy::unwrap_used)]
    fn clone(&self) -> Self {
        let inner = self.inner.lock().unwrap();
        Self {
            inner: SMutex::new(VecCursorInner {
                rows: inner.rows.clone(),
                index: inner.index,
            }),
        }
    }
}

#[async_trait]
impl RSCursor for VecCursor {
    async fn next(&self) -> RS<Option<TupleRow>> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "range cursor lock poisoned"))?;
        if inner.index >= inner.rows.len() {
            return Ok(None);
        }
        let row = inner.rows[inner.index].clone();
        inner.index += 1;
        Ok(Some(row))
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;

    fn make_cursor(rows: Vec<TupleRow>) -> VecCursor {
        VecCursor {
            inner: SMutex::new(VecCursorInner { rows, index: 0 }),
        }
    }

    fn row(values: Vec<Vec<u8>>) -> TupleRow {
        TupleRow::new(values)
    }

    #[test]
    fn vec_cursor_next_returns_rows_in_order() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let cursor = make_cursor(vec![row(vec![vec![1]]), row(vec![vec![2]])]);
            assert_eq!(cursor.next().await.unwrap().unwrap().get(0), Some(vec![1]));
            assert_eq!(cursor.next().await.unwrap().unwrap().get(0), Some(vec![2]));
        })
        .unwrap()
    }

    #[test]
    fn vec_cursor_next_after_last_returns_none() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let cursor = make_cursor(vec![row(vec![vec![1]])]);
            assert!(cursor.next().await.unwrap().is_some());
            assert!(cursor.next().await.unwrap().is_none());
            assert!(cursor.next().await.unwrap().is_none());
        })
        .unwrap()
    }

    #[test]
    fn vec_cursor_empty_returns_none() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let cursor = make_cursor(vec![]);
            assert!(cursor.next().await.unwrap().is_none());
        })
        .unwrap()
    }

    #[test]
    fn vec_cursor_clone_maintains_independent_index() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let cursor = make_cursor(vec![row(vec![vec![1]]), row(vec![vec![2]])]);
            assert!(cursor.next().await.unwrap().is_some());

            let cloned = cursor.clone();
            assert!(cursor.next().await.unwrap().is_some());
            assert!(cursor.next().await.unwrap().is_none());

            assert!(cloned.next().await.unwrap().is_some());
            assert!(cloned.next().await.unwrap().is_none());
        })
        .unwrap()
    }

    #[test]
    fn vec_cursor_one_row() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let cursor = make_cursor(vec![row(vec![vec![42]])]);
            assert_eq!(cursor.next().await.unwrap().unwrap().get(0), Some(vec![42]));
            assert!(cursor.next().await.unwrap().is_none());
        })
        .unwrap()
    }
}
