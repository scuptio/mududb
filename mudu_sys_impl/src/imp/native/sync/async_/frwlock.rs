//! Async read-write lock built from `SMutex` + `ANotify`.
//!
//! `ARwLock` (tokio-based) shares the Tokio waiter/waker machinery that we
//! observed stalling on the custom io_uring/task-runtime path (see
//! `futures_mutex.rs`). This implementation only uses primitives that are
//! already proven on that path (`SMutex` for the state machine and `ANotify`
//! for wakeup, the same pair the WAL group-commit waiter uses).
//!
//! Admission is fair: readers only wait for an *active* writer, writers wait
//! for all readers and the active writer to leave. (A writer-preferring
//! variant was measured to hurt read-heavy statement latency: new readers
//! bunched behind merely-queued writers.)

use super::notify::ANotify;
use crate::sync::SMutex;

#[derive(Default)]
struct FRwLockState {
    readers: usize,
    writer: bool,
    waiting_writers: usize,
}

/// Async read-write lock; see module docs.
pub struct FRwLock {
    state: SMutex<FRwLockState>,
    notify: ANotify,
}

/// RAII read access; releasing the last reader wakes a waiting writer.
pub struct FRwLockReadGuard<'a> {
    lock: &'a FRwLock,
}

/// RAII exclusive access; releasing wakes all waiters.
pub struct FRwLockWriteGuard<'a> {
    lock: &'a FRwLock,
}

/// Tracks a queued writer so a cancelled `write()` future cannot leak the
/// waiting-writer count (which would block readers forever).
struct WriterTicket<'a> {
    lock: &'a FRwLock,
    active: bool,
}

impl FRwLock {
    /// Create a new unlocked `FRwLock`.
    pub fn new() -> Self {
        Self {
            state: SMutex::new(FRwLockState::default()),
            notify: ANotify::new(),
        }
    }

    fn lock_state(&self) -> crate::sync::std_mutex::SMutexGuard<'_, FRwLockState> {
        #[expect(
            clippy::expect_used,
            reason = "a poisoned FRwLock state is a programming error"
        )]
        self.state.lock().expect("frwlock state lock poisoned")
    }

    fn try_acquire_read(&self) -> Option<FRwLockReadGuard<'_>> {
        let mut state = self.lock_state();
        // Fair admission: readers only wait for an *active* writer. Blocking
        // new readers behind merely-queued writers measurably hurt statement
        // latency on read-heavy paths (every point read would bunch behind
        // commit writers), and write gaps between interleaved reads keep
        // writer starvation away in practice.
        if state.writer {
            return None;
        }
        state.readers += 1;
        Some(FRwLockReadGuard { lock: self })
    }

    fn try_acquire_write(&self) -> Option<FRwLockWriteGuard<'_>> {
        let mut state = self.lock_state();
        if state.writer || state.readers > 0 {
            return None;
        }
        state.writer = true;
        state.waiting_writers -= 1;
        Some(FRwLockWriteGuard { lock: self })
    }

    /// Acquire shared read access, waiting while a writer holds or waits for
    /// the lock.
    pub async fn read(&self) -> FRwLockReadGuard<'_> {
        loop {
            if let Some(guard) = self.try_acquire_read() {
                return guard;
            }
            // Clear-and-recheck before parking so a notify racing the clear
            // cannot be missed (the condition re-check observes its state
            // change, and a later notify re-arms the sticky flag).
            self.notify.clear_signal();
            if let Some(guard) = self.try_acquire_read() {
                return guard;
            }
            self.notify.notified().await;
        }
    }

    /// Acquire exclusive write access, waiting until no reader or writer
    /// holds the lock.
    pub async fn write(&self) -> FRwLockWriteGuard<'_> {
        let mut ticket = WriterTicket::new(self);
        loop {
            if let Some(guard) = self.try_acquire_write() {
                ticket.defuse();
                return guard;
            }
            self.notify.clear_signal();
            if let Some(guard) = self.try_acquire_write() {
                ticket.defuse();
                return guard;
            }
            self.notify.notified().await;
        }
    }
}

impl Default for FRwLock {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> WriterTicket<'a> {
    fn new(lock: &'a FRwLock) -> Self {
        lock.lock_state().waiting_writers += 1;
        Self { lock, active: true }
    }

    fn defuse(&mut self) {
        self.active = false;
    }
}

impl Drop for WriterTicket<'_> {
    fn drop(&mut self) {
        if self.active {
            let mut state = self.lock.lock_state();
            state.waiting_writers -= 1;
            if state.waiting_writers == 0 && !state.writer {
                // No writer remains queued: readers may proceed.
                drop(state);
                self.lock.notify.notify_waiters();
            }
        }
    }
}

impl Drop for FRwLockReadGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.lock.lock_state();
        state.readers -= 1;
        if state.readers == 0 && state.waiting_writers > 0 {
            drop(state);
            self.lock.notify.notify_waiters();
        }
    }
}

impl Drop for FRwLockWriteGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.lock.lock_state();
        state.writer = false;
        drop(state);
        // Wake everyone: queued writers re-contend, readers re-check the
        // writer-preference condition.
        self.lock.notify.notify_waiters();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test(flavor = "current_thread")]
    async fn concurrent_readers_are_admitted_together() {
        let lock = FRwLock::new();
        let first = lock.read().await;
        let second = timeout(Duration::from_millis(50), lock.read())
            .await
            .expect("second reader must not wait for the first");
        drop(first);
        drop(second);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn writer_waits_for_readers_queued_writer_does_not_block_readers() {
        let lock = Arc::new(FRwLock::new());
        let reader = lock.read().await;

        let write_lock = lock.clone();
        let writer = tokio::spawn(async move {
            let _guard = write_lock.write().await;
        });
        // Let the writer queue up.
        tokio::task::yield_now().await;
        // Fair admission: a merely-queued writer does not block new readers.
        let another_reader = timeout(Duration::from_millis(50), lock.read())
            .await
            .expect("reader must not wait behind a queued writer");
        drop(another_reader);

        drop(reader);
        timeout(Duration::from_millis(500), writer)
            .await
            .expect("writer join")
            .expect("writer must acquire after last reader leaves");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reader_waits_for_active_writer() {
        let lock = Arc::new(FRwLock::new());
        let writer = lock.write().await;
        let blocked_reader = timeout(Duration::from_millis(30), lock.read()).await;
        assert!(blocked_reader.is_err());
        drop(writer);
        let _reader = timeout(Duration::from_millis(500), lock.read())
            .await
            .expect("reader must acquire after writer releases");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn readers_resume_after_writer_releases() {
        let lock = Arc::new(FRwLock::new());
        let writer = lock.write().await;
        let read_lock = lock.clone();
        let reader = tokio::spawn(async move {
            let _guard = read_lock.read().await;
        });
        tokio::task::yield_now().await;
        drop(writer);
        timeout(Duration::from_millis(500), reader)
            .await
            .expect("reader join")
            .expect("reader must acquire after writer releases");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_writer_does_not_block_readers() {
        let lock = Arc::new(FRwLock::new());
        let reader = lock.read().await;
        let write_lock = lock.clone();
        let writer = tokio::spawn(async move {
            let _guard = write_lock.write().await;
        });
        tokio::task::yield_now().await;
        writer.abort();
        let _ = writer.await;
        drop(reader);
        // The cancelled writer must not leak its queue slot.
        let _guard = timeout(Duration::from_millis(500), lock.read())
            .await
            .expect("reader must acquire after writer cancellation");
    }
}
