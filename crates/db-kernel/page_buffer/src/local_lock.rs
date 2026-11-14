//! Fair, cancellation-safe coordination between tasks on one partition thread.
//! RefCell enforces byte access; the queue only coordinates suspension. No atomics,
//! blocking mutexes, spinning, or unsafe concurrent access are involved.
use std::{
    cell::{Ref, RefCell, RefMut},
    collections::VecDeque,
    future::poll_fn,
    ops::{Deref, DerefMut},
    rc::Rc,
    task::{Poll, Waker},
};

struct Waiter {
    wake: RefCell<Option<Waker>>,
}

/// Thread-confined read/write coordination. Guards may span awaits only when the
/// operation intentionally excludes other local tasks (for example maintenance).
#[derive(Default)]
pub struct LocalRwLock<T> {
    data: RefCell<T>,
    queue: RefCell<VecDeque<Rc<Waiter>>>,
}
impl<T> LocalRwLock<T> {
    /// Creates an unlocked local value.
    pub fn new(value: T) -> Self {
        Self {
            data: RefCell::new(value),
            queue: RefCell::new(VecDeque::new()),
        }
    }
    fn wake_front(&self) {
        let wake = self
            .queue
            .borrow()
            .front()
            .and_then(|w| w.wake.borrow().clone());
        if let Some(wake) = wake {
            wake.wake();
        }
    }
    /// Borrows immediately unless a writer or an earlier waiter excludes access.
    pub fn try_read(&self) -> Option<LocalReadGuard<'_, T>> {
        if !self.queue.borrow().is_empty() {
            return None;
        }
        self.data.try_borrow().ok().map(|value| LocalReadGuard {
            value: Some(value),
            lock: self,
        })
    }
    /// Borrows exclusively without waiting or bypassing queued tasks.
    pub fn try_write(&self) -> Option<LocalWriteGuard<'_, T>> {
        if !self.queue.borrow().is_empty() {
            return None;
        }
        self.data
            .try_borrow_mut()
            .ok()
            .map(|value| LocalWriteGuard {
                value: Some(value),
                lock: self,
            })
    }
    /// Suspends locally until shared access is available, preserving FIFO fairness.
    pub async fn read(&self) -> LocalReadGuard<'_, T> {
        if let Some(guard) = self.try_read() {
            return guard;
        }
        let ticket = Ticket::new(self);
        poll_fn(|cx| {
            *ticket.waiter.wake.borrow_mut() = Some(cx.waker().clone());
            if ticket.first() {
                if let Ok(value) = self.data.try_borrow() {
                    return Poll::Ready(LocalReadGuard {
                        value: Some(value),
                        lock: self,
                    });
                }
            }
            Poll::Pending
        })
        .await
    }
    /// Suspends locally until exclusive access is available.
    pub async fn write(&self) -> LocalWriteGuard<'_, T> {
        if let Some(guard) = self.try_write() {
            return guard;
        }
        let ticket = Ticket::new(self);
        poll_fn(|cx| {
            *ticket.waiter.wake.borrow_mut() = Some(cx.waker().clone());
            if ticket.first() {
                if let Ok(value) = self.data.try_borrow_mut() {
                    return Poll::Ready(LocalWriteGuard {
                        value: Some(value),
                        lock: self,
                    });
                }
            }
            Poll::Pending
        })
        .await
    }
    /// Exclusive acquisition for callers that do not need shared access.
    pub async fn lock(&self) -> LocalWriteGuard<'_, T> {
        self.write().await
    }
    /// Immediate exclusive acquisition.
    pub fn try_lock(&self) -> Option<LocalWriteGuard<'_, T>> {
        self.try_write()
    }
}
struct Ticket<'a, T> {
    lock: &'a LocalRwLock<T>,
    waiter: Rc<Waiter>,
}
impl<'a, T> Ticket<'a, T> {
    fn new(lock: &'a LocalRwLock<T>) -> Self {
        let waiter = Rc::new(Waiter {
            wake: RefCell::new(None),
        });
        lock.queue.borrow_mut().push_back(waiter.clone());
        Self { lock, waiter }
    }
    fn first(&self) -> bool {
        self.lock
            .queue
            .borrow()
            .front()
            .is_some_and(|w| Rc::ptr_eq(w, &self.waiter))
    }
}
impl<T> Drop for Ticket<'_, T> {
    fn drop(&mut self) {
        self.lock
            .queue
            .borrow_mut()
            .retain(|w| !Rc::ptr_eq(w, &self.waiter));
        self.lock.wake_front();
    }
}
/// Shared local borrow, released before waking the next task.
pub struct LocalReadGuard<'a, T> {
    value: Option<Ref<'a, T>>,
    lock: &'a LocalRwLock<T>,
}
impl<T> Deref for LocalReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value.as_deref().unwrap_or_else(|| unreachable!())
    }
}
impl<T> Drop for LocalReadGuard<'_, T> {
    fn drop(&mut self) {
        self.value.take();
        self.lock.wake_front();
    }
}
/// Exclusive local borrow, released before waking the next task.
pub struct LocalWriteGuard<'a, T> {
    value: Option<RefMut<'a, T>>,
    lock: &'a LocalRwLock<T>,
}
impl<T> Deref for LocalWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value.as_deref().unwrap_or_else(|| unreachable!())
    }
}
impl<T> DerefMut for LocalWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value.as_deref_mut().unwrap_or_else(|| unreachable!())
    }
}
impl<T> Drop for LocalWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.value.take();
        self.lock.wake_front();
    }
}
