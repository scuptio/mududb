//! Wake the synchronous bootstrap driver when an external async wait completes.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;
use std::task::{Wake, Waker};

struct MailboxWaker(OwnedFd);

impl Wake for MailboxWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if let Err(err) = mudu_sys::sync::blocking::notify_eventfd(self.0.as_raw_fd()) {
            tracing::error!("bootstrap mailbox wake failed: {err}");
        }
    }
}

pub(super) fn mailbox_waker(fd: i32) -> RS<Waker> {
    // Async primitives can retain a waker after the future completes. Own a
    // duplicate so a late wake cannot write to a closed/reused descriptor.
    let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if duplicate < 0 {
        return Err(mudu_error!(
            ErrorCode::Internal,
            "duplicate bootstrap mailbox fd",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: fcntl returned a new descriptor owned exclusively by this waker.
    Ok(Waker::from(Arc::new(MailboxWaker(unsafe {
        OwnedFd::from_raw_fd(duplicate)
    }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bootstrap_waker_delivers_cross_thread_and_repeated_wakes() -> RS<()> {
        let fd = mudu_sys::sync::blocking::eventfd()?;
        // SAFETY: eventfd returned a new descriptor owned by this test.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let waker = mailbox_waker(fd.as_raw_fd())?;
        let remote = waker.clone();
        mudu_sys::task::sync::spawn_thread(move || remote.wake())?
            .join()
            .expect("wake thread panicked");
        assert_eq!(mudu_sys::sync::blocking::read_eventfd(fd.as_raw_fd())?, 1);
        waker.wake_by_ref();
        assert_eq!(mudu_sys::sync::blocking::read_eventfd(fd.as_raw_fd())?, 1);
        // The retained waker owns the eventfd even after its original owner
        // closes the descriptor; a late wake must remain safe.
        drop(fd);
        waker.wake();
        Ok(())
    }
}
